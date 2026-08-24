use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalDragSession {
    pub token: u64,
    pub source_drop_id: String,
    pub source_file_ids: Vec<u64>,
    pub target_drop_id: Option<String>,
}

#[derive(Debug, Default)]
struct InternalDragStateInner {
    next_token: u64,
    active: Option<InternalDragSession>,
}

#[derive(Debug, Clone, Default)]
pub struct InternalDragState {
    inner: Arc<Mutex<InternalDragStateInner>>,
}

impl InternalDragState {
    pub fn begin(&self, source_drop_id: String, source_file_ids: Vec<u64>) -> Result<u64, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Failed to lock internal drag state".to_string())?;
        inner.next_token = inner.next_token.wrapping_add(1).max(1);
        let token = inner.next_token;
        inner.active = Some(InternalDragSession {
            token,
            source_drop_id,
            source_file_ids,
            target_drop_id: None,
        });
        Ok(token)
    }

    pub fn record_target(&self, target_drop_id: &str) -> Result<bool, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Failed to lock internal drag state".to_string())?;
        let Some(session) = inner.active.as_mut() else {
            return Ok(false);
        };
        session.target_drop_id = Some(target_drop_id.to_string());
        Ok(true)
    }

    pub fn finish(&self, token: u64) -> Result<Option<InternalDragSession>, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Failed to lock internal drag state".to_string())?;
        if inner.active.as_ref().map(|session| session.token) != Some(token) {
            return Ok(None);
        }
        Ok(inner.active.take())
    }
}

#[cfg(test)]
mod tests {
    use super::InternalDragState;

    #[test]
    fn lifecycle_records_target_and_clears_the_session() {
        let state = InternalDragState::default();
        let token = state.begin("source".into(), vec![1, 2]).unwrap();

        assert!(state.record_target("target").unwrap());
        let session = state.finish(token).unwrap().unwrap();

        assert_eq!(session.source_drop_id, "source");
        assert_eq!(session.source_file_ids, vec![1, 2]);
        assert_eq!(session.target_drop_id.as_deref(), Some("target"));
        assert!(!state.record_target("another").unwrap());
    }

    #[test]
    fn stale_completion_does_not_clear_a_newer_drag() {
        let state = InternalDragState::default();
        let first = state.begin("first".into(), vec![1]).unwrap();
        let second = state.begin("second".into(), vec![2]).unwrap();

        assert!(state.finish(first).unwrap().is_none());
        assert!(state.record_target("target").unwrap());
        assert_eq!(
            state.finish(second).unwrap().unwrap().source_drop_id,
            "second"
        );
    }

    #[test]
    fn cancelled_drag_is_removed_without_a_target() {
        let state = InternalDragState::default();
        let token = state.begin("source".into(), vec![7]).unwrap();

        let session = state.finish(token).unwrap().unwrap();

        assert!(session.target_drop_id.is_none());
        assert!(!state.record_target("target").unwrap());
    }
}
