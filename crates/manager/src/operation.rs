use std::sync::Arc;

use domain::OperationRecord;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::store::{ManagerStore, StoreError};

#[derive(Clone)]
pub struct OperationQueue {
    store: ManagerStore,
    execution: Arc<Mutex<()>>,
}

impl OperationQueue {
    pub fn new(store: ManagerStore) -> Self {
        Self {
            store,
            execution: Arc::new(Mutex::new(())),
        }
    }

    pub fn enqueue(
        &self,
        kind: &str,
        env_id: Option<&str>,
        label: &str,
        generation: u64,
    ) -> Result<OperationRecord, StoreError> {
        self.store.create_operation(kind, env_id, label, generation)
    }

    pub async fn acquire(&self) -> OwnedMutexGuard<()> {
        self.execution.clone().lock_owned().await
    }

    pub fn start(&self, id: &str, message: &str) -> Result<OperationRecord, StoreError> {
        self.store
            .transition_operation(id, "running", message, None)
    }

    pub fn succeed(&self, id: &str, message: &str) -> Result<OperationRecord, StoreError> {
        self.store
            .transition_operation(id, "succeeded", message, None)
    }

    pub fn fail(&self, id: &str, code: &str, message: &str) -> Result<OperationRecord, StoreError> {
        self.store
            .transition_operation(id, "failed", message, Some(code))
    }

    pub fn cancel(&self, id: &str, message: &str) -> Result<OperationRecord, StoreError> {
        self.store
            .transition_operation(id, "cancelled", message, None)
    }
}
