use serde::{Deserialize, Serialize};

use crate::engine::{
    AgentTeam, PlanStatus, RecoveryPolicy, TeamDisplayMode, TeamMessage, TeamMessagePriority,
    TeamTask, TeamUsage,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerRequest {
    Ping,
    ListSessions,
    CreateSession {
        name: String,
    },
    AttachSession {
        name: String,
    },
    TeamList,
    TeamCreate {
        team_id: String,
        mode: TeamDisplayMode,
        delegation_only: bool,
    },
    TeamAddMember {
        team_id: String,
        name: String,
        model: String,
        require_plan_approval: bool,
        is_lead: bool,
    },
    TeamAddTask {
        team_id: String,
        title: String,
        deps: Vec<usize>,
        touched_files: Vec<String>,
    },
    TeamSubmitPlan {
        team_id: String,
        member_id: usize,
        plan: String,
    },
    TeamClaimTask {
        team_id: String,
        member_id: usize,
        task_id: usize,
    },
    TeamCompleteTask {
        team_id: String,
        member_id: usize,
        task_id: usize,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
    },
    TeamSetPlanStatus {
        team_id: String,
        member_id: usize,
        status: PlanStatus,
    },
    TeamAutoClaim {
        team_id: String,
        member_id: usize,
    },
    TeamSetMode {
        team_id: String,
        mode: TeamDisplayMode,
    },
    TeamSetDelegationOnly {
        team_id: String,
        delegation_only: bool,
    },
    TeamSetRecoveryPolicy {
        team_id: String,
        recovery_policy: RecoveryPolicy,
    },
    TeamRemoveMember {
        team_id: String,
        member_id: usize,
        reason: String,
    },
    TeamRestartMember {
        team_id: String,
        member_id: usize,
    },
    TeamCleanup {
        team_id: String,
    },
    TeamPruneTerminated {
        team_id: String,
    },
    TeamPostMessage {
        team_id: String,
        from_member: Option<usize>,
        to_member: Option<usize>,
        text: String,
        priority: TeamMessagePriority,
    },
    TeamListMessages {
        team_id: String,
        viewer_member: Option<usize>,
        unread_only: bool,
    },
    TeamMarkMessageRead {
        team_id: String,
        member_id: usize,
        message_id: usize,
    },
    TeamUsage {
        team_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResponse {
    pub ok: bool,
    pub message: String,
    pub sessions: Vec<String>,
    #[serde(default)]
    pub teams: Vec<AgentTeam>,
    #[serde(default)]
    pub tasks: Vec<TeamTask>,
    #[serde(default)]
    pub messages: Vec<TeamMessage>,
    #[serde(default)]
    pub usage: Option<TeamUsage>,
}

impl ServerResponse {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
            sessions: Vec::new(),
            teams: Vec::new(),
            tasks: Vec::new(),
            messages: Vec::new(),
            usage: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
            sessions: Vec::new(),
            teams: Vec::new(),
            tasks: Vec::new(),
            messages: Vec::new(),
            usage: None,
        }
    }
}
