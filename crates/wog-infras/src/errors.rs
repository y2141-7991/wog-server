#[derive(Clone, Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("An error occurred when obtaining database connection")]
    DatabaseConnectionError,
    #[error("Username or email already exists {0}")]
    ExistedDataError(String),
    #[error("The requested resource was not found in the database")]
    NotFound,
    #[error("A unique constraint violation occurred")]
    UniqueViolation,
    #[error("A check constraint violation occurred: {0}")]
    CheckViolation(String),
    #[error("An error occurred when generating typed SQL query")]
    QueryGenerationFailed,
    #[error("A validation error occurred: {0}")]
    ValidationError(String),
    #[error("An unknown error occurred: {0}")]
    Others(String),
}

impl From<sqlx::Error> for DatabaseError {
    fn from(value: sqlx::Error) -> Self {
        match value {
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                DatabaseError::DatabaseConnectionError
            }
            sqlx::Error::RowNotFound => DatabaseError::NotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => DatabaseError::UniqueViolation,
                Some("23514") => {
                    let msg = db_err.constraint().unwrap_or("unknown").to_string();
                    DatabaseError::CheckViolation(msg)
                }
                _ => DatabaseError::Others(db_err.message().to_string()),
            },
            sqlx::Error::ColumnNotFound(col) => {
                DatabaseError::Others(format!("Column not found: {col}"))
            }
            sqlx::Error::ColumnDecode { source, index } => {
                DatabaseError::Others(format!("Decode error at column {index}: {source}"))
            }
            other => DatabaseError::Others(other.to_string()),
        }
    }
}
