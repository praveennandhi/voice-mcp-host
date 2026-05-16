use crate::agent_types::AgentSessionTurn;
use crate::app_state::AppState;

pub fn append_turns(state: &AppState, user: &str, assistant: &str, mode: &str) {
    let mut session = state.agent_session.lock().unwrap();
    session.push(AgentSessionTurn {
        role: "user".into(),
        content: user.trim().to_string(),
        mode: None,
    });
    session.push(AgentSessionTurn {
        role: "assistant".into(),
        content: assistant.trim().to_string(),
        mode: Some(mode.into()),
    });
    trim_session(&mut session);
}

pub fn format_history(history: &[AgentSessionTurn]) -> String {
    let turns = history
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|turn| {
            let mode = turn
                .mode
                .as_deref()
                .map(|m| format!(" ({m})"))
                .unwrap_or_default();
            format!("{}{}: {}", turn.role, mode, turn.content)
        })
        .collect::<Vec<_>>();

    if turns.is_empty() {
        "No prior turns in this session.".into()
    } else {
        turns.join("\n")
    }
}

pub fn with_tool_result(
    history: &[AgentSessionTurn],
    command: &str,
    summary: &str,
    content: &str,
) -> Vec<AgentSessionTurn> {
    let mut next = history.to_vec();
    next.push(AgentSessionTurn {
        role: "user".into(),
        content: command.trim().to_string(),
        mode: None,
    });
    next.push(AgentSessionTurn {
        role: "tool".into(),
        content: format!("{summary}\n\n{content}"),
        mode: Some("workspace".into()),
    });
    next
}

fn trim_session(session: &mut Vec<AgentSessionTurn>) {
    if session.len() > 40 {
        let remove_count = session.len() - 40;
        session.drain(0..remove_count);
    }
}
