/// COBOL source format.
///
/// Controls how the lexer interprets column positions:
/// - `Fixed`: traditional 80-column format with areas A/B (columns 1-6 sequence,
///   7 indicator, 8-72 program text, 73-80 identification).
/// - `Free`: no column restrictions; the entire line is program text.
/// - `Variable`: like fixed format but without the right margin at column 72.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SourceFormat {
    #[default]
    Fixed,
    Free,
    Variable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_fixed() {
        assert_eq!(SourceFormat::default(), SourceFormat::Fixed);
    }
}
