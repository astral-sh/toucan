/// Generated text separated from the banner and caller-provided Rust.
///
/// Formatters can process declarations without inspecting or rewriting either
/// the banner or raw lines. The ordinary generator appends raw lines; adapters
/// may place them before declarations instead.
#[derive(Debug)]
pub struct SourceParts {
    pub banner: String,
    pub declarations: String,
    pub raw_lines: Vec<String>,
}

impl SourceParts {
    /// Assemble the ordinary generator's banner, declarations, and raw lines.
    pub fn into_string(self) -> String {
        let mut source = self.declarations;
        source.insert_str(0, &self.banner);
        for line in self.raw_lines {
            source.push_str(&line);
            source.push('\n');
        }
        source
    }
}
