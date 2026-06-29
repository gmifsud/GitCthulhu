//! cache_adapter
//! Implements GraphCache using `redb` for embedded storage and `bincode` for fast binary serialization.

use core_domain::{DagNode, DomainError, GraphCache};
use redb::{Database, TableDefinition};
use std::path::Path;

const DAG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("dag_cache");

pub struct RedbGraphCache {
    db: Database,
}

impl RedbGraphCache {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, DomainError> {
        let db = Database::create(path)
            .map_err(|e| DomainError::Unknown(format!("Failed to open redb database: {}", e)))?;
        
        // Ensure table exists
        let write_txn = db.begin_write()
            .map_err(|e| DomainError::Unknown(format!("Failed to start write txn: {}", e)))?;
        {
            let _table = write_txn.open_table(DAG_TABLE)
                .map_err(|e| DomainError::Unknown(format!("Failed to open table: {}", e)))?;
        }
        write_txn.commit()
            .map_err(|e| DomainError::Unknown(format!("Failed to commit table creation: {}", e)))?;
            
        Ok(Self { db })
    }
}

impl GraphCache for RedbGraphCache {
    fn get_dag(&self, cache_key: &str) -> Result<Option<Vec<DagNode>>, DomainError> {
        let read_txn = self.db.begin_read()
            .map_err(|e| DomainError::Unknown(format!("Failed to start read txn: {}", e)))?;
            
        let table = read_txn.open_table(DAG_TABLE)
            .map_err(|e| DomainError::Unknown(format!("Failed to open read table: {}", e)))?;
            
        let value = table.get(cache_key)
            .map_err(|e| DomainError::Unknown(format!("Failed to get cache key: {}", e)))?;
            
        if let Some(access) = value {
            let bytes = access.value();
            let dag: Vec<DagNode> = bincode::deserialize(bytes)
                .map_err(|e| DomainError::Unknown(format!("Failed to deserialize DAG: {}", e)))?;
            Ok(Some(dag))
        } else {
            Ok(None)
        }
    }

    fn store_dag(&self, cache_key: &str, dag: &[DagNode]) -> Result<(), DomainError> {
        let bytes = bincode::serialize(dag)
            .map_err(|e| DomainError::Unknown(format!("Failed to serialize DAG: {}", e)))?;
            
        let write_txn = self.db.begin_write()
            .map_err(|e| DomainError::Unknown(format!("Failed to start write txn: {}", e)))?;
            
        {
            let mut table = write_txn.open_table(DAG_TABLE)
                .map_err(|e| DomainError::Unknown(format!("Failed to open write table: {}", e)))?;
            table.insert(cache_key, bytes.as_slice())
                .map_err(|e| DomainError::Unknown(format!("Failed to insert into cache: {}", e)))?;
        }
        
        write_txn.commit()
            .map_err(|e| DomainError::Unknown(format!("Failed to commit DAG insertion: {}", e)))?;
            
        Ok(())
    }
}
