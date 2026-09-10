use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database busy (attempt {attempt}/{max_attempts})")]
    Busy { attempt: u32, max_attempts: u32 },

    #[error("Schema creation failed: {0}")]
    SchemaError(String),

    #[error("Query execution failed: {query}\n  Cause: {cause}")]
    QueryError { query: String, cause: String },

    #[error("CSV generation failed for table {table}: {cause}")]
    CsvError { table: String, cause: String },

    #[error("Connection pool exhausted")]
    PoolExhausted,

    #[error(transparent)]
    Core(#[from] code_explorer_core::error::CoreError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;
