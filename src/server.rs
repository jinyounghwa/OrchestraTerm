use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use crate::engine::EngineState;
use crate::protocol::{ServerRequest, ServerResponse};

pub fn server_addr() -> String {
    std::env::var("ORCHESTRATERM_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:7899".to_string())
}

pub fn run_server() -> Result<()> {
    let listener = TcpListener::bind(server_addr()).with_context(|| "failed to bind tcp server")?;

    let state = Arc::new(Mutex::new(EngineState::load_or_default()));
    if let Ok(st) = state.lock() {
        let _ = st.save();
    }

    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let state = state.clone();
        std::thread::spawn(move || {
            let _ = handle_client(stream, state);
        });
    }

    Ok(())
}

fn handle_client(stream: TcpStream, state: Arc<Mutex<EngineState>>) -> Result<()> {
    let mut writer = stream
        .try_clone()
        .with_context(|| "failed to clone stream")?;
    let mut reader = BufReader::new(stream);

    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .with_context(|| "failed to read request")?;
        if n == 0 {
            break;
        }

        let req: ServerRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(err) => {
                let raw =
                    serde_json::to_string(&ServerResponse::err(format!("invalid request: {err}")))?;
                writer.write_all(raw.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                continue;
            }
        };

        let mut guard = state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let mut resp = match req {
            ServerRequest::Ping => ServerResponse::ok("pong"),
            ServerRequest::ListSessions => {
                let mut r = ServerResponse::ok("ok");
                r.sessions = guard.list_sessions();
                r
            }
            ServerRequest::CreateSession { name } => {
                guard.create_session(&name);
                ServerResponse::ok(format!("created session: {name}"))
            }
            ServerRequest::AttachSession { name } => {
                guard.set_active_session(&name);
                ServerResponse::ok(format!("active session: {name}"))
            }
            ServerRequest::TeamList => {
                let mut r = ServerResponse::ok("ok");
                r.teams = guard.teams.values().cloned().collect();
                r
            }
            ServerRequest::TeamCreate {
                team_id,
                mode,
                delegation_only,
            } => match guard.create_team(&team_id, mode, delegation_only) {
                Ok(()) => ServerResponse::ok(format!("created team: {team_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamAddMember {
                team_id,
                name,
                model,
                require_plan_approval,
                is_lead,
            } => match guard.add_member(&team_id, &name, &model, require_plan_approval, is_lead) {
                Ok(member) => {
                    ServerResponse::ok(format!("member added: {} (id={})", member.name, member.id))
                }
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamAddTask {
                team_id,
                title,
                deps,
                touched_files,
            } => match guard.add_task(&team_id, &title, deps, touched_files) {
                Ok(task) => {
                    let mut r =
                        ServerResponse::ok(format!("task added: {} (id={})", task.title, task.id));
                    r.tasks = vec![task];
                    r
                }
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamSubmitPlan {
                team_id,
                member_id,
                plan,
            } => match guard.submit_plan(&team_id, member_id, &plan) {
                Ok(()) => ServerResponse::ok(format!("plan submitted for member: {member_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamClaimTask {
                team_id,
                member_id,
                task_id,
            } => match guard.claim_task(&team_id, member_id, task_id) {
                Ok(()) => ServerResponse::ok(format!("task claimed: {task_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamCompleteTask {
                team_id,
                member_id,
                task_id,
                input_tokens,
                output_tokens,
                cost_usd,
            } => match guard.complete_task(
                &team_id,
                member_id,
                task_id,
                input_tokens,
                output_tokens,
                cost_usd,
            ) {
                Ok(()) => ServerResponse::ok(format!("task completed: {task_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamSetPlanStatus {
                team_id,
                member_id,
                status,
            } => match guard.set_plan_status(&team_id, member_id, status) {
                Ok(()) => ServerResponse::ok(format!("member plan status updated: {member_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamAutoClaim { team_id, member_id } => {
                match guard.auto_claim_next_task(&team_id, member_id) {
                    Ok(Some(task_id)) => {
                        ServerResponse::ok(format!("auto-claimed task: {task_id}"))
                    }
                    Ok(None) => ServerResponse::ok("no claimable task"),
                    Err(err) => ServerResponse::err(err.to_string()),
                }
            }
            ServerRequest::TeamSetMode { team_id, mode } => {
                match guard.set_team_mode(&team_id, mode) {
                    Ok(()) => ServerResponse::ok("team mode updated"),
                    Err(err) => ServerResponse::err(err.to_string()),
                }
            }
            ServerRequest::TeamSetDelegationOnly {
                team_id,
                delegation_only,
            } => match guard.set_delegation_only(&team_id, delegation_only) {
                Ok(()) => ServerResponse::ok("delegation mode updated"),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamSetRecoveryPolicy {
                team_id,
                recovery_policy,
            } => match guard.set_recovery_policy(&team_id, recovery_policy) {
                Ok(()) => ServerResponse::ok("recovery policy updated"),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamRemoveMember {
                team_id,
                member_id,
                reason,
            } => match guard.remove_member(&team_id, member_id, &reason) {
                Ok(()) => ServerResponse::ok(format!("member terminated: {member_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamRestartMember { team_id, member_id } => {
                match guard.restart_member(&team_id, member_id) {
                    Ok(()) => ServerResponse::ok(format!("member restarted: {member_id}")),
                    Err(err) => ServerResponse::err(err.to_string()),
                }
            }
            ServerRequest::TeamPruneTerminated { team_id } => {
                match guard.prune_terminated(&team_id) {
                    Ok(()) => ServerResponse::ok("terminated members pruned"),
                    Err(err) => ServerResponse::err(err.to_string()),
                }
            }
            ServerRequest::TeamCleanup { team_id } => match guard.cleanup_team(&team_id) {
                Ok(()) => ServerResponse::ok(format!("team removed: {team_id}")),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamPostMessage {
                team_id,
                from_member,
                to_member,
                text,
                priority,
            } => match guard.post_message(&team_id, from_member, to_member, &text, priority) {
                Ok(msg) => {
                    let mut r = ServerResponse::ok(format!("message posted: {}", msg.id));
                    r.messages = vec![msg];
                    r
                }
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamListMessages {
                team_id,
                viewer_member,
                unread_only,
            } => match guard.team_messages(&team_id, viewer_member, unread_only) {
                Ok(messages) => {
                    let mut r = ServerResponse::ok("ok");
                    r.messages = messages;
                    r
                }
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamMarkMessageRead {
                team_id,
                member_id,
                message_id,
            } => match guard.mark_message_read(&team_id, member_id, message_id) {
                Ok(()) => ServerResponse::ok("message marked as read"),
                Err(err) => ServerResponse::err(err.to_string()),
            },
            ServerRequest::TeamUsage { team_id } => match guard.team_usage(&team_id) {
                Ok(usage) => {
                    let mut r = ServerResponse::ok("ok");
                    r.usage = Some(usage);
                    r
                }
                Err(err) => ServerResponse::err(err.to_string()),
            },
        };

        let _ = guard.save();
        resp.sessions = guard.list_sessions();
        if resp.teams.is_empty() {
            resp.teams = guard.teams.values().cloned().collect();
        }
        if resp.messages.is_empty() {
            resp.messages = Vec::new();
        }

        let raw = serde_json::to_string(&resp)?;
        writer.write_all(raw.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }

    Ok(())
}

pub fn send_request(req: &ServerRequest) -> Result<ServerResponse> {
    let mut stream =
        TcpStream::connect(server_addr()).with_context(|| "failed to connect server")?;

    let raw = serde_json::to_string(req)?;
    stream.write_all(raw.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let resp = serde_json::from_str(line.trim())?;
    Ok(resp)
}
