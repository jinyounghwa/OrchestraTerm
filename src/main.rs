use anyhow::Result;
use clap::{Parser, Subcommand};
use eframe::egui;
use orchestraterm::engine::{PlanStatus, RecoveryPolicy, TeamDisplayMode, TeamMessagePriority};
use orchestraterm::gui::OrchestraApp;
use orchestraterm::protocol::ServerRequest;
use orchestraterm::server;

#[derive(Debug, Parser)]
#[command(
    name = "orchestraterm",
    version,
    about = "tmux-like GUI terminal engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Gui,
    Server {
        #[command(subcommand)]
        command: ServerCmd,
    },
    Team {
        #[command(subcommand)]
        command: TeamCmd,
    },
}

#[derive(Debug, Subcommand)]
enum ServerCmd {
    Start,
    Ping,
    Sessions,
    Create { name: String },
    Attach { name: String },
}

#[derive(Debug, Subcommand)]
enum TeamCmd {
    List,
    Create {
        team_id: String,
        #[arg(long, default_value = "in_process")]
        mode: String,
        #[arg(long, default_value_t = false)]
        delegation_only: bool,
    },
    AddMember {
        team_id: String,
        name: String,
        #[arg(long, default_value = "gpt-5")]
        model: String,
        #[arg(long, default_value_t = false)]
        require_plan_approval: bool,
        #[arg(long, default_value_t = false)]
        lead: bool,
    },
    AddTask {
        team_id: String,
        title: String,
        #[arg(long, value_delimiter = ',')]
        deps: Vec<usize>,
        #[arg(long = "files", value_delimiter = ',')]
        touched_files: Vec<String>,
    },
    SubmitPlan {
        team_id: String,
        member_id: usize,
        plan: String,
    },
    Claim {
        team_id: String,
        member_id: usize,
        task_id: usize,
    },
    Done {
        team_id: String,
        member_id: usize,
        task_id: usize,
        #[arg(long, default_value_t = 0)]
        input_tokens: u64,
        #[arg(long, default_value_t = 0)]
        output_tokens: u64,
        #[arg(long, default_value_t = 0.0)]
        cost_usd: f64,
    },
    Plan {
        team_id: String,
        member_id: usize,
        #[arg(long)]
        status: String,
    },
    AutoClaim {
        team_id: String,
        member_id: usize,
    },
    SetMode {
        team_id: String,
        #[arg(long)]
        mode: String,
    },
    SetDelegation {
        team_id: String,
        #[arg(long)]
        delegation_only: bool,
    },
    SetRecovery {
        team_id: String,
        #[arg(long)]
        policy: String,
    },
    RemoveMember {
        team_id: String,
        member_id: usize,
        #[arg(long, default_value = "terminated by operator")]
        reason: String,
    },
    RestartMember {
        team_id: String,
        member_id: usize,
    },
    Cleanup {
        team_id: String,
    },
    PruneTerminated {
        team_id: String,
    },
    Message {
        team_id: String,
        #[arg(long)]
        from_member: Option<usize>,
        #[arg(long)]
        to_member: Option<usize>,
        #[arg(long, default_value = "normal")]
        priority: String,
        text: String,
    },
    Messages {
        team_id: String,
        #[arg(long)]
        viewer_member: Option<usize>,
        #[arg(long, default_value_t = false)]
        unread_only: bool,
    },
    ReadMessage {
        team_id: String,
        member_id: usize,
        message_id: usize,
    },
    Usage {
        team_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Command::Gui) => run_gui(),
        Some(Command::Server { command }) => run_server_cli(command),
        Some(Command::Team { command }) => run_team_cli(command),
    }
}

fn run_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OrchestraTerm")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([1080.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "OrchestraTerm",
        options,
        Box::new(|cc| {
            configure_fonts(&cc.egui_ctx);
            Ok(Box::new(OrchestraApp::new()))
        }),
    )
    .map_err(|e| anyhow::anyhow!("failed to launch GUI: {e}"))?;

    Ok(())
}

fn run_server_cli(command: ServerCmd) -> Result<()> {
    match command {
        ServerCmd::Start => server::run_server(),
        ServerCmd::Ping => {
            let resp = server::send_request(&ServerRequest::Ping)?;
            println!("{}", resp.message);
            Ok(())
        }
        ServerCmd::Sessions => {
            let resp = server::send_request(&ServerRequest::ListSessions)?;
            for name in resp.sessions {
                println!("{name}");
            }
            Ok(())
        }
        ServerCmd::Create { name } => {
            let resp = server::send_request(&ServerRequest::CreateSession { name })?;
            println!("{}", resp.message);
            Ok(())
        }
        ServerCmd::Attach { name } => {
            let resp = server::send_request(&ServerRequest::AttachSession { name })?;
            println!("{}", resp.message);
            Ok(())
        }
    }
}

fn run_team_cli(command: TeamCmd) -> Result<()> {
    match command {
        TeamCmd::List => {
            let resp = server::send_request(&ServerRequest::TeamList)?;
            for team in resp.teams {
                println!(
                    "team={} mode={:?} delegation_only={} members={} tasks={}",
                    team.id,
                    team.mode,
                    team.delegation_only,
                    team.members.len(),
                    team.tasks.len()
                );
            }
            Ok(())
        }
        TeamCmd::Create {
            team_id,
            mode,
            delegation_only,
        } => {
            let mode = parse_mode(&mode)?;
            let resp = server::send_request(&ServerRequest::TeamCreate {
                team_id,
                mode,
                delegation_only,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::AddMember {
            team_id,
            name,
            model,
            require_plan_approval,
            lead,
        } => {
            let resp = server::send_request(&ServerRequest::TeamAddMember {
                team_id,
                name,
                model,
                require_plan_approval,
                is_lead: lead,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::AddTask {
            team_id,
            title,
            deps,
            touched_files,
        } => {
            let resp = server::send_request(&ServerRequest::TeamAddTask {
                team_id,
                title,
                deps,
                touched_files,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::SubmitPlan {
            team_id,
            member_id,
            plan,
        } => {
            let resp = server::send_request(&ServerRequest::TeamSubmitPlan {
                team_id,
                member_id,
                plan,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Claim {
            team_id,
            member_id,
            task_id,
        } => {
            let resp = server::send_request(&ServerRequest::TeamClaimTask {
                team_id,
                member_id,
                task_id,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Done {
            team_id,
            member_id,
            task_id,
            input_tokens,
            output_tokens,
            cost_usd,
        } => {
            let resp = server::send_request(&ServerRequest::TeamCompleteTask {
                team_id,
                member_id,
                task_id,
                input_tokens,
                output_tokens,
                cost_usd,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Plan {
            team_id,
            member_id,
            status,
        } => {
            let status = parse_plan_status(&status)?;
            let resp = server::send_request(&ServerRequest::TeamSetPlanStatus {
                team_id,
                member_id,
                status,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::AutoClaim { team_id, member_id } => {
            let resp = server::send_request(&ServerRequest::TeamAutoClaim { team_id, member_id })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::SetMode { team_id, mode } => {
            let mode = parse_mode(&mode)?;
            let resp = server::send_request(&ServerRequest::TeamSetMode { team_id, mode })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::SetDelegation {
            team_id,
            delegation_only,
        } => {
            let resp = server::send_request(&ServerRequest::TeamSetDelegationOnly {
                team_id,
                delegation_only,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::SetRecovery { team_id, policy } => {
            let recovery_policy = parse_recovery_policy(&policy)?;
            let resp = server::send_request(&ServerRequest::TeamSetRecoveryPolicy {
                team_id,
                recovery_policy,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::RemoveMember {
            team_id,
            member_id,
            reason,
        } => {
            let resp = server::send_request(&ServerRequest::TeamRemoveMember {
                team_id,
                member_id,
                reason,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::RestartMember { team_id, member_id } => {
            let resp =
                server::send_request(&ServerRequest::TeamRestartMember { team_id, member_id })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Cleanup { team_id } => {
            let resp = server::send_request(&ServerRequest::TeamCleanup { team_id })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::PruneTerminated { team_id } => {
            let resp = server::send_request(&ServerRequest::TeamPruneTerminated { team_id })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Message {
            team_id,
            from_member,
            to_member,
            priority,
            text,
        } => {
            let priority = parse_message_priority(&priority)?;
            let resp = server::send_request(&ServerRequest::TeamPostMessage {
                team_id,
                from_member,
                to_member,
                text,
                priority,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Messages {
            team_id,
            viewer_member,
            unread_only,
        } => {
            let resp = server::send_request(&ServerRequest::TeamListMessages {
                team_id,
                viewer_member,
                unread_only,
            })?;
            for m in resp.messages {
                println!(
                    "#{} p={:?} from={:?} to={:?} read_by={:?} {}",
                    m.id, m.priority, m.from_member, m.to_member, m.read_by, m.text
                );
            }
            Ok(())
        }
        TeamCmd::ReadMessage {
            team_id,
            member_id,
            message_id,
        } => {
            let resp = server::send_request(&ServerRequest::TeamMarkMessageRead {
                team_id,
                member_id,
                message_id,
            })?;
            println!("{}", resp.message);
            Ok(())
        }
        TeamCmd::Usage { team_id } => {
            let resp = server::send_request(&ServerRequest::TeamUsage { team_id })?;
            if let Some(usage) = resp.usage {
                println!(
                    "input_tokens={} output_tokens={} cost_usd={:.6} active_tasks={}",
                    usage.input_tokens, usage.output_tokens, usage.cost_usd, usage.active_tasks
                );
            } else {
                println!("{}", resp.message);
            }
            Ok(())
        }
    }
}

fn parse_mode(v: &str) -> Result<TeamDisplayMode> {
    match v {
        "in_process" => Ok(TeamDisplayMode::InProcess),
        "split_pane" => Ok(TeamDisplayMode::SplitPane),
        "auto" => Ok(TeamDisplayMode::Auto),
        _ => Err(anyhow::anyhow!("invalid mode: {v}")),
    }
}

fn parse_plan_status(v: &str) -> Result<PlanStatus> {
    match v {
        "planning" => Ok(PlanStatus::Planning),
        "approved" => Ok(PlanStatus::Approved),
        "rejected" => Ok(PlanStatus::Rejected),
        _ => Err(anyhow::anyhow!("invalid plan status: {v}")),
    }
}

fn parse_recovery_policy(v: &str) -> Result<RecoveryPolicy> {
    match v {
        "auto_reassign" => Ok(RecoveryPolicy::AutoReassign),
        "manual" => Ok(RecoveryPolicy::Manual),
        _ => Err(anyhow::anyhow!("invalid recovery policy: {v}")),
    }
}

fn parse_message_priority(v: &str) -> Result<TeamMessagePriority> {
    match v {
        "low" => Ok(TeamMessagePriority::Low),
        "normal" => Ok(TeamMessagePriority::Normal),
        "high" => Ok(TeamMessagePriority::High),
        "urgent" => Ok(TeamMessagePriority::Urgent),
        _ => Err(anyhow::anyhow!("invalid priority: {v}")),
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Keep ASCII/ANSI block rendering crisp with a true monospace primary face.
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/Menlo.ttc") {
        fonts
            .font_data
            .insert("menlo".to_owned(), egui::FontData::from_owned(bytes).into());
        if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            mono.insert(0, "menlo".to_owned());
        }
    }

    // Korean fallback: appended to avoid replacing monospace pixel-grid metrics.
    for path in [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/AppleGothic.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "korean".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("korean".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("korean".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}
