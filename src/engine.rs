use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    pub sessions: BTreeMap<String, SessionState>,
    pub active_session: Option<String>,
    #[serde(default)]
    pub teams: BTreeMap<String, AgentTeam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub name: String,
    pub windows: Vec<WindowState>,
    pub active_window: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub id: usize,
    pub title: String,
    pub panes: Vec<PaneState>,
    pub active_pane: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneState {
    pub id: usize,
    pub title: String,
    pub cwd: Option<String>,
}

impl Default for EngineState {
    fn default() -> Self {
        let mut sessions = BTreeMap::new();
        sessions.insert(
            "default".to_string(),
            SessionState {
                name: "default".to_string(),
                windows: vec![WindowState {
                    id: 0,
                    title: "Window 0".to_string(),
                    panes: vec![PaneState {
                        id: 0,
                        title: "Pane 0".to_string(),
                        cwd: None,
                    }],
                    active_pane: 0,
                }],
                active_window: 0,
            },
        );

        Self {
            sessions,
            active_session: Some("default".to_string()),
            teams: BTreeMap::new(),
        }
    }
}

impl EngineState {
    pub fn create_session(&mut self, name: &str) {
        if self.sessions.contains_key(name) {
            return;
        }
        self.sessions.insert(
            name.to_string(),
            SessionState {
                name: name.to_string(),
                windows: vec![WindowState {
                    id: 0,
                    title: "Window 0".to_string(),
                    panes: vec![PaneState {
                        id: 0,
                        title: "Pane 0".to_string(),
                        cwd: None,
                    }],
                    active_pane: 0,
                }],
                active_window: 0,
            },
        );
        self.active_session = Some(name.to_string());
    }

    pub fn list_sessions(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn set_active_session(&mut self, name: &str) {
        if self.sessions.contains_key(name) {
            self.active_session = Some(name.to_string());
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = state_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        fs::write(&path, raw).with_context(|| format!("failed to write state: {}", path.display()))
    }

    pub fn load_or_default() -> Self {
        let Ok(path) = state_file_path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn create_team(
        &mut self,
        team_id: &str,
        mode: TeamDisplayMode,
        delegation_only: bool,
    ) -> Result<()> {
        if self.teams.contains_key(team_id) {
            anyhow::bail!("team already exists: {team_id}");
        }
        self.teams.insert(
            team_id.to_string(),
            AgentTeam {
                id: team_id.to_string(),
                mode,
                delegation_only,
                recovery_policy: RecoveryPolicy::AutoReassign,
                members: Vec::new(),
                tasks: Vec::new(),
                messages: Vec::new(),
                next_member_id: 0,
                next_task_id: 0,
                next_message_id: 0,
            },
        );
        Ok(())
    }

    pub fn add_member(
        &mut self,
        team_id: &str,
        name: &str,
        model: &str,
        require_plan_approval: bool,
        is_lead: bool,
    ) -> Result<MemberState> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member = MemberState {
            id: team.next_member_id,
            name: name.to_string(),
            model: model.to_string(),
            is_lead,
            status: MemberStatus::Active,
            terminated_at: None,
            termination_reason: None,
            require_plan_approval,
            plan_status: if require_plan_approval {
                PlanStatus::Planning
            } else {
                PlanStatus::Approved
            },
            latest_plan: None,
            plan_updated_at: None,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        };
        team.next_member_id += 1;
        team.members.push(member.clone());
        Ok(member)
    }

    pub fn add_task(
        &mut self,
        team_id: &str,
        title: &str,
        deps: Vec<usize>,
        touched_files: Vec<String>,
    ) -> Result<TeamTask> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;

        for dep in &deps {
            if !team.tasks.iter().any(|t| t.id == *dep) {
                anyhow::bail!("unknown dependency task id: {dep}");
            }
        }

        let now = now_unix_secs();
        let mut task = TeamTask {
            id: team.next_task_id,
            title: title.to_string(),
            status: TaskStatus::Pending,
            assignee: None,
            deps,
            touched_files,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            created_at: now,
            updated_at: now,
        };
        team.next_task_id += 1;
        team.tasks.push(task.clone());
        Self::refresh_task_blocking(team)?;
        if let Some(updated) = team.tasks.iter().find(|t| t.id == task.id) {
            task = updated.clone();
        }
        Ok(task)
    }

    pub fn submit_plan(&mut self, team_id: &str, member_id: usize, plan: &str) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member = team
            .members
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))?;
        Self::ensure_member_active(member)?;
        member.latest_plan = Some(plan.to_string());
        member.plan_updated_at = Some(now_unix_secs());
        member.plan_status = PlanStatus::Planning;
        Ok(())
    }

    pub fn claim_task(&mut self, team_id: &str, member_id: usize, task_id: usize) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member_idx = team
            .members
            .iter()
            .position(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))?;
        let member = &team.members[member_idx];
        Self::ensure_member_active(member)?;
        Self::ensure_member_allowed_to_execute(team, member)?;
        Self::ensure_plan_gate(member)?;

        Self::refresh_task_blocking(team)?;

        let task_idx = team
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("unknown task id: {task_id}"))?;
        if !matches!(team.tasks[task_idx].status, TaskStatus::Pending) {
            anyhow::bail!("task is not pending: {task_id}");
        }
        if Self::has_file_conflict(team, task_idx)? {
            anyhow::bail!("task has file conflict with another in-progress task: {task_id}");
        }
        team.tasks[task_idx].status = TaskStatus::InProgress;
        team.tasks[task_idx].assignee = Some(member_id);
        team.tasks[task_idx].updated_at = now_unix_secs();
        Ok(())
    }

    pub fn complete_task(
        &mut self,
        team_id: &str,
        member_id: usize,
        task_id: usize,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
    ) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member = Self::find_member(team, member_id)?;
        Self::ensure_member_active(member)?;

        let task = team
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("unknown task id: {task_id}"))?;
        if !matches!(task.status, TaskStatus::InProgress) {
            anyhow::bail!("task is not in progress: {task_id}");
        }
        if task.assignee != Some(member_id) {
            anyhow::bail!("member {member_id} is not assignee for task {task_id}");
        }

        task.status = TaskStatus::Done;
        task.updated_at = now_unix_secs();
        task.input_tokens += input_tokens;
        task.output_tokens += output_tokens;
        task.cost_usd += cost_usd.max(0.0);

        let member = Self::find_member(team, member_id)?;
        member.input_tokens += input_tokens;
        member.output_tokens += output_tokens;
        member.cost_usd += cost_usd.max(0.0);

        Self::refresh_task_blocking(team)?;
        Ok(())
    }

    pub fn auto_claim_next_task(
        &mut self,
        team_id: &str,
        member_id: usize,
    ) -> Result<Option<usize>> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member_idx = team
            .members
            .iter()
            .position(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))?;
        let member = &team.members[member_idx];
        Self::ensure_member_active(member)?;
        Self::ensure_member_allowed_to_execute(team, member)?;
        Self::ensure_plan_gate(member)?;
        Self::refresh_task_blocking(team)?;

        let mut claimed = None;
        for idx in 0..team.tasks.len() {
            if !matches!(team.tasks[idx].status, TaskStatus::Pending) {
                continue;
            }
            if Self::has_file_conflict(team, idx)? {
                continue;
            }
            team.tasks[idx].status = TaskStatus::InProgress;
            team.tasks[idx].assignee = Some(member_id);
            team.tasks[idx].updated_at = now_unix_secs();
            claimed = Some(team.tasks[idx].id);
            break;
        }
        Ok(claimed)
    }

    pub fn set_plan_status(
        &mut self,
        team_id: &str,
        member_id: usize,
        status: PlanStatus,
    ) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member = team
            .members
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))?;
        Self::ensure_member_active(member)?;
        member.plan_status = status;
        member.plan_updated_at = Some(now_unix_secs());
        Ok(())
    }

    pub fn set_delegation_only(&mut self, team_id: &str, delegation_only: bool) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        team.delegation_only = delegation_only;
        Ok(())
    }

    pub fn set_team_mode(&mut self, team_id: &str, mode: TeamDisplayMode) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        team.mode = mode;
        Ok(())
    }

    pub fn set_recovery_policy(
        &mut self,
        team_id: &str,
        recovery_policy: RecoveryPolicy,
    ) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        team.recovery_policy = recovery_policy;
        Ok(())
    }

    pub fn remove_member(&mut self, team_id: &str, member_id: usize, reason: &str) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;

        let member = team
            .members
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))?;
        member.status = MemberStatus::Terminated;
        member.terminated_at = Some(now_unix_secs());
        member.termination_reason = Some(reason.to_string());

        for task in &mut team.tasks {
            if task.assignee == Some(member_id) && !matches!(task.status, TaskStatus::Done) {
                task.assignee = None;
                task.updated_at = now_unix_secs();
                task.status = match team.recovery_policy {
                    RecoveryPolicy::AutoReassign => TaskStatus::Pending,
                    RecoveryPolicy::Manual => TaskStatus::Blocked,
                };
            }
        }

        Self::refresh_task_blocking(team)?;
        Ok(())
    }

    pub fn restart_member(&mut self, team_id: &str, member_id: usize) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member = team
            .members
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))?;
        member.status = MemberStatus::Active;
        member.terminated_at = None;
        member.termination_reason = None;
        Ok(())
    }

    pub fn cleanup_team(&mut self, team_id: &str) -> Result<()> {
        if self.teams.remove(team_id).is_none() {
            anyhow::bail!("unknown team: {team_id}");
        }
        Ok(())
    }

    pub fn prune_terminated(&mut self, team_id: &str) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let active_ids: BTreeSet<usize> = team
            .members
            .iter()
            .filter(|m| matches!(m.status, MemberStatus::Active))
            .map(|m| m.id)
            .collect();
        team.members
            .retain(|m| matches!(m.status, MemberStatus::Active));
        for msg in &mut team.messages {
            msg.read_by.retain(|id| active_ids.contains(id));
        }
        Ok(())
    }

    pub fn post_message(
        &mut self,
        team_id: &str,
        from_member: Option<usize>,
        to_member: Option<usize>,
        text: &str,
        priority: TeamMessagePriority,
    ) -> Result<TeamMessage> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        if let Some(from) = from_member {
            let member = Self::find_member(team, from)?;
            Self::ensure_member_active(member)?;
        }
        if let Some(to) = to_member {
            let member = Self::find_member(team, to)?;
            Self::ensure_member_active(member)?;
        }
        let msg = TeamMessage {
            id: team.next_message_id,
            from_member,
            to_member,
            text: text.to_string(),
            priority,
            created_at: now_unix_secs(),
            read_by: Vec::new(),
        };
        team.next_message_id += 1;
        team.messages.push(msg.clone());
        Ok(msg)
    }

    pub fn team_messages(
        &self,
        team_id: &str,
        viewer_member: Option<usize>,
        unread_only: bool,
    ) -> Result<Vec<TeamMessage>> {
        let team = self
            .teams
            .get(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        if let Some(viewer) = viewer_member {
            let _ = team
                .members
                .iter()
                .find(|m| m.id == viewer)
                .ok_or_else(|| anyhow::anyhow!("unknown member id: {viewer}"))?;
        }
        let mut out = Vec::new();
        for msg in &team.messages {
            let visible = match viewer_member {
                Some(viewer) => msg.to_member.is_none() || msg.to_member == Some(viewer),
                None => true,
            };
            if !visible {
                continue;
            }
            if unread_only {
                if let Some(viewer) = viewer_member {
                    if msg.read_by.contains(&viewer) {
                        continue;
                    }
                }
            }
            out.push(msg.clone());
        }
        Ok(out)
    }

    pub fn mark_message_read(
        &mut self,
        team_id: &str,
        member_id: usize,
        message_id: usize,
    ) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let member = Self::find_member(team, member_id)?;
        Self::ensure_member_active(member)?;
        let message = team
            .messages
            .iter_mut()
            .find(|m| m.id == message_id)
            .ok_or_else(|| anyhow::anyhow!("unknown message id: {message_id}"))?;
        if !message.read_by.contains(&member_id) {
            message.read_by.push(member_id);
        }
        Ok(())
    }

    pub fn team_usage(&self, team_id: &str) -> Result<TeamUsage> {
        let team = self
            .teams
            .get(team_id)
            .ok_or_else(|| anyhow::anyhow!("unknown team: {team_id}"))?;
        let mut usage = TeamUsage::default();
        for task in &team.tasks {
            usage.input_tokens += task.input_tokens;
            usage.output_tokens += task.output_tokens;
            usage.cost_usd += task.cost_usd;
        }
        usage.active_tasks = team
            .tasks
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::InProgress))
            .count() as u64;
        Ok(usage)
    }

    fn find_member(team: &mut AgentTeam, member_id: usize) -> Result<&mut MemberState> {
        team.members
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member id: {member_id}"))
    }

    fn ensure_member_active(member: &MemberState) -> Result<()> {
        if !matches!(member.status, MemberStatus::Active) {
            anyhow::bail!("member is not active: {}", member.id);
        }
        Ok(())
    }

    fn ensure_member_allowed_to_execute(team: &AgentTeam, member: &MemberState) -> Result<()> {
        if team.delegation_only && member.is_lead {
            anyhow::bail!("delegation-only mode: lead member cannot execute tasks");
        }
        Ok(())
    }

    fn ensure_plan_gate(member: &MemberState) -> Result<()> {
        if member.require_plan_approval && !matches!(member.plan_status, PlanStatus::Approved) {
            anyhow::bail!("plan approval required for member {}", member.id);
        }
        Ok(())
    }

    fn has_file_conflict(team: &AgentTeam, candidate_idx: usize) -> Result<bool> {
        let candidate = team
            .tasks
            .get(candidate_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid task index"))?;
        if candidate.touched_files.is_empty() {
            return Ok(false);
        }
        let cand: BTreeSet<String> = candidate
            .touched_files
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if cand.is_empty() {
            return Ok(false);
        }
        for (idx, other) in team.tasks.iter().enumerate() {
            if idx == candidate_idx || !matches!(other.status, TaskStatus::InProgress) {
                continue;
            }
            for path in &other.touched_files {
                if cand.contains(&path.trim().to_ascii_lowercase()) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn refresh_task_blocking(team: &mut AgentTeam) -> Result<()> {
        let done: BTreeSet<usize> = team
            .tasks
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Done))
            .map(|t| t.id)
            .collect();
        let existing_task_ids: BTreeSet<usize> = team.tasks.iter().map(|t| t.id).collect();
        for task in &mut team.tasks {
            if matches!(task.status, TaskStatus::InProgress | TaskStatus::Done) {
                continue;
            }
            for dep in &task.deps {
                if !existing_task_ids.contains(dep) {
                    anyhow::bail!("task {} has missing dependency {}", task.id, dep);
                }
            }
            let blocked = task.deps.iter().any(|dep| !done.contains(dep));
            let next_status = if blocked {
                TaskStatus::Blocked
            } else {
                TaskStatus::Pending
            };
            if task.status != next_status {
                task.status = next_status;
                task.updated_at = now_unix_secs();
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeam {
    pub id: String,
    pub mode: TeamDisplayMode,
    #[serde(default)]
    pub delegation_only: bool,
    #[serde(default)]
    pub recovery_policy: RecoveryPolicy,
    #[serde(default)]
    pub members: Vec<MemberState>,
    #[serde(default)]
    pub tasks: Vec<TeamTask>,
    #[serde(default)]
    pub messages: Vec<TeamMessage>,
    #[serde(default)]
    pub next_member_id: usize,
    #[serde(default)]
    pub next_task_id: usize,
    #[serde(default)]
    pub next_message_id: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamDisplayMode {
    InProcess,
    SplitPane,
    Auto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPolicy {
    AutoReassign,
    Manual,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self::AutoReassign
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberState {
    pub id: usize,
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub is_lead: bool,
    #[serde(default)]
    pub status: MemberStatus,
    #[serde(default)]
    pub terminated_at: Option<u64>,
    #[serde(default)]
    pub termination_reason: Option<String>,
    #[serde(default)]
    pub require_plan_approval: bool,
    #[serde(default = "default_plan_status")]
    pub plan_status: PlanStatus,
    #[serde(default)]
    pub latest_plan: Option<String>,
    #[serde(default)]
    pub plan_updated_at: Option<u64>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Active,
    Terminated,
}

impl Default for MemberStatus {
    fn default() -> Self {
        Self::Active
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Planning,
    Approved,
    Rejected,
}

fn default_plan_status() -> PlanStatus {
    PlanStatus::Planning
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    pub id: usize,
    pub title: String,
    pub status: TaskStatus,
    pub assignee: Option<usize>,
    #[serde(default)]
    pub deps: Vec<usize>,
    #[serde(default)]
    pub touched_files: Vec<String>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMessage {
    pub id: usize,
    pub from_member: Option<usize>,
    pub to_member: Option<usize>,
    pub text: String,
    #[serde(default)]
    pub priority: TeamMessagePriority,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub read_by: Vec<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamMessagePriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Default for TeamMessagePriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Blocked,
    InProgress,
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub active_tasks: u64,
}

pub fn state_file_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("engine-state.json"))
}

pub fn runtime_dir() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var("ORCHESTRATERM_RUNTIME_DIR") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let fallback = std::env::current_dir()
        .with_context(|| "failed to get current dir")?
        .join(".orchestraterm-runtime");
    fs::create_dir_all(&fallback)
        .with_context(|| format!("failed to create runtime dir: {}", fallback.display()))?;
    Ok(fallback)
}

fn now_unix_secs() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_list_sessions() {
        let mut st = EngineState::default();
        st.create_session("dev");
        let names = st.list_sessions();
        assert!(names.contains(&"default".to_string()));
        assert!(names.contains(&"dev".to_string()));
    }

    #[test]
    fn delegation_only_blocks_lead_claim() {
        let mut st = EngineState::default();
        st.create_team("t1", TeamDisplayMode::Auto, true).unwrap();
        let lead = st.add_member("t1", "lead", "gpt-5", false, true).unwrap();
        st.add_task("t1", "Task A", vec![], vec!["src/main.rs".to_string()])
            .unwrap();
        let err = st.claim_task("t1", lead.id, 0).unwrap_err().to_string();
        assert!(err.contains("delegation-only"));
    }

    #[test]
    fn plan_gate_blocks_claim_until_approved() {
        let mut st = EngineState::default();
        st.create_team("t1", TeamDisplayMode::InProcess, false)
            .unwrap();
        let worker = st.add_member("t1", "w", "gpt-5", true, false).unwrap();
        st.add_task("t1", "Task A", vec![], vec![]).unwrap();
        assert!(st.claim_task("t1", worker.id, 0).is_err());
        st.submit_plan("t1", worker.id, "plan").unwrap();
        st.set_plan_status("t1", worker.id, PlanStatus::Approved)
            .unwrap();
        assert!(st.claim_task("t1", worker.id, 0).is_ok());
    }

    #[test]
    fn task_dependency_transitions_from_blocked_to_pending() {
        let mut st = EngineState::default();
        st.create_team("t1", TeamDisplayMode::SplitPane, false)
            .unwrap();
        let worker = st.add_member("t1", "w", "gpt-5", false, false).unwrap();
        let t0 = st.add_task("t1", "A", vec![], vec![]).unwrap();
        let t1 = st.add_task("t1", "B", vec![t0.id], vec![]).unwrap();
        assert!(matches!(t1.status, TaskStatus::Blocked));
        st.claim_task("t1", worker.id, t0.id).unwrap();
        st.complete_task("t1", worker.id, t0.id, 100, 20, 0.01)
            .unwrap();
        let team = st.teams.get("t1").unwrap();
        let next = team.tasks.iter().find(|t| t.id == t1.id).unwrap();
        assert!(matches!(next.status, TaskStatus::Pending));
    }
}
