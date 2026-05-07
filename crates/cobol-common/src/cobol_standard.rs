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

impl CobolStandard {
    pub fn parse_cli(value: &str) -> Option<Self> {
        let normalized = value
            .chars()
            .filter(|ch| *ch != '-' && *ch != '_' && !ch.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        match normalized.as_str() {
            "85" | "cobol85" | "cobol1985" | "1985" => Some(Self::Cobol85),
            "2002" | "cobol2002" => Some(Self::Cobol2002),
            "2014" | "cobol2014" => Some(Self::Cobol2014),
            "2023" | "cobol2023" => Some(Self::Cobol2023),
            _ => None,
        }
    }

    pub fn cli_values() -> &'static str {
        "cobol85, cobol2002, cobol2014, cobol2023"
    }

    pub fn as_cli_str(self) -> &'static str {
        match self {
            Self::Cobol85 => "cobol85",
            Self::Cobol2002 => "cobol2002",
            Self::Cobol2014 => "cobol2014",
            Self::Cobol2023 => "cobol2023",
        }
    }

    pub fn allows(self, required: Self) -> bool {
        self.rank() >= required.rank()
    }

    fn rank(self) -> u8 {
        match self {
            Self::Cobol85 => 85,
            Self::Cobol2002 => 102,
            Self::Cobol2014 => 114,
            Self::Cobol2023 => 123,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_cobol2023() {
        assert_eq!(CobolStandard::default(), CobolStandard::Cobol2023);
    }

    #[test]
    fn test_parse_cli_accepts_common_spellings() {
        assert_eq!(
            CobolStandard::parse_cli("cobol85"),
            Some(CobolStandard::Cobol85)
        );
        assert_eq!(
            CobolStandard::parse_cli("COBOL-2002"),
            Some(CobolStandard::Cobol2002)
        );
        assert_eq!(
            CobolStandard::parse_cli("2014"),
            Some(CobolStandard::Cobol2014)
        );
        assert_eq!(
            CobolStandard::parse_cli("cobol_2023"),
            Some(CobolStandard::Cobol2023)
        );
        assert_eq!(CobolStandard::parse_cli("unknown"), None);
    }

    #[test]
    fn test_standard_ordering() {
        assert!(CobolStandard::Cobol2014.allows(CobolStandard::Cobol2002));
        assert!(!CobolStandard::Cobol85.allows(CobolStandard::Cobol2014));
    }
}
