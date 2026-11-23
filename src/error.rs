use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum SightError {
    #[error("{0}")]
    InvalidArgs(String),
    #[error("{0}")]
    TargetNotFound(String),
    #[error("window is minimized")]
    WindowMinimized,
    #[error("window is protected from capture")]
    WindowProtected,
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Internal(String),
}

impl SightError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgs(_) => "INVALID_ARGS",
            Self::TargetNotFound(_) => "TARGET_NOT_FOUND",
            Self::WindowMinimized => "WINDOW_MINIMIZED",
            Self::WindowProtected => "WINDOW_PROTECTED",
            Self::Permission(_) => "PERMISSION_DENIED",
            Self::Timeout(_) => "TIMEOUT",
            Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidArgs(_) => 1,
            Self::TargetNotFound(_) => 2,
            Self::WindowMinimized | Self::WindowProtected => 3,
            Self::Permission(_) => 4,
            Self::Timeout(_) => 5,
            Self::Internal(_) => 6,
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::InvalidArgs(_) => "run `sight --help` for usage",
            Self::TargetNotFound(_) => "run `sight list` to see available targets",
            Self::WindowMinimized => "restore the window and retry",
            Self::WindowProtected => "the window opted out of capture (display affinity)",
            Self::Permission(_) => "run with sufficient privileges for the target process",
            Self::Timeout(_) => "increase --timeout or --settle",
            Self::Internal(_) => "run `sight doctor` to check the environment",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_exit_codes_and_hints_are_stable() {
        let cases: [(SightError, &str, i32); 7] = [
            (SightError::InvalidArgs("x".into()), "INVALID_ARGS", 1),
            (
                SightError::TargetNotFound("x".into()),
                "TARGET_NOT_FOUND",
                2,
            ),
            (SightError::WindowMinimized, "WINDOW_MINIMIZED", 3),
            (SightError::WindowProtected, "WINDOW_PROTECTED", 3),
            (SightError::Permission("x".into()), "PERMISSION_DENIED", 4),
            (SightError::Timeout("x".into()), "TIMEOUT", 5),
            (SightError::Internal("x".into()), "INTERNAL", 6),
        ];
        for (e, code, exit) in cases {
            assert_eq!(e.code(), code);
            assert_eq!(e.exit_code(), exit);
            assert!(!e.hint().is_empty());
        }
    }
}
