use std::io::{Read, Write};
use std::sync::{Arc, Mutex, mpsc};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

pub struct PaneTerminal {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
    child: Box<dyn portable_pty::Child + Send>,
}

impl PaneTerminal {
    pub fn spawn(initial_dir: Option<&str>) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 48,
                cols: 160,
                pixel_width: 0,
                pixel_height: 0,
            })
            .with_context(|| "failed to open pty")?;

        let mut cmd = CommandBuilder::new("/bin/zsh");
        cmd.arg("-i");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "OrchestraTerm");
        cmd.env("LC_CTYPE", "UTF-8");

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| "failed to spawn shell")?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .with_context(|| "failed to clone pty reader")?;
        let writer = pair
            .master
            .take_writer()
            .with_context(|| "failed to take pty writer")?;

        let master = Arc::new(Mutex::new(pair.master));
        let (tx, rx) = mpsc::channel::<Vec<u8>>();

        std::thread::spawn(move || {
            let mut buf = [0_u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx.send(buf[..n].to_vec());
                    }
                    Err(_) => break,
                }
            }
        });

        let mut terminal = Self {
            writer: Arc::new(Mutex::new(writer)),
            master,
            output_rx: rx,
            child,
        };

        terminal.send_line("clear")?;
        if let Some(dir) = initial_dir {
            terminal.send_line(&format!("cd {}", shell_quote(dir)))?;
        }

        Ok(terminal)
    }

    pub fn send_line(&mut self, text: &str) -> Result<()> {
        self.write_bytes(text.as_bytes())?;
        self.write_bytes(b"\n")?;
        Ok(())
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> Result<()> {
        let mut guard = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("failed to lock pty writer"))?;
        guard
            .write_all(data)
            .with_context(|| "failed to write bytes to pty")?;
        guard
            .flush()
            .with_context(|| "failed to flush pty writer")?;
        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        let guard = self
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("failed to lock pty master"))?;
        guard
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .with_context(|| "failed to resize pty")?;
        Ok(())
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for PaneTerminal {
    fn drop(&mut self) {
        self.kill();
    }
}

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}
