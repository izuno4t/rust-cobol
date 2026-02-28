/// COBOL language standard version.
///
/// Controls which language features are available during compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CobolStandard {
    Cobol85,
    Cobol2002,
    Cobol2014,
    #[default]
    Cobol2023,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_cobol2023() {
        assert_eq!(CobolStandard::default(), CobolStandard::Cobol2023);
    }
}
