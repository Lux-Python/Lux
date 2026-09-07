use super::theme::{Glyphs, Style};

/// A node in a visual dependency graph or resolution tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyNode {
    /// Package name.
    pub name: String,
    /// Exact resolved version.
    pub version: String,
    /// Original constraint/specifier (e.g. `">=2.0"`).
    pub constraint: Option<String>,
    /// Nested child dependencies.
    pub children: Vec<Self>,
}

impl DependencyNode {
    /// Construct a new dependency tree leaf or branch.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            constraint: None,
            children: Vec::new(),
        }
    }

    /// Add an original constraint specifier to this node.
    #[must_use]
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraint = Some(constraint.into());
        self
    }

    /// Add a child dependency node.
    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }
}

/// Render an entire dependency tree to the terminal with standard branch characters.
pub fn render_dependency_tree(root: &DependencyNode) {
    let root_name = Style::bold(&root.name);
    let root_ver = Style::cyan(&format!("v{}", root.version));
    println!("{root_name} {root_ver}");

    for (i, child) in root.children.iter().enumerate() {
        let is_last = i == root.children.len() - 1;
        render_node(child, "", is_last);
    }
}

fn render_node(node: &DependencyNode, prefix: &str, is_last: bool) {
    let branch = if is_last {
        Glyphs::LAST_BRANCH
    } else {
        Glyphs::BRANCH
    };

    let branch_styled = Style::dim(branch);
    let name_styled = Style::bold(&node.name);
    let ver_styled = Style::cyan(&format!("v{}", node.version));

    let constraint_styled = node.constraint.as_ref().map_or_else(String::new, |c| {
        format!(" {}", Style::dim(&format!("({c})")))
    });

    println!("{prefix}{branch_styled}{name_styled} {ver_styled}{constraint_styled}");

    let extension = if is_last {
        Glyphs::EMPTY
    } else {
        Glyphs::VERTICAL
    };
    let new_prefix = format!("{prefix}{}", Style::dim(extension));

    for (i, child) in node.children.iter().enumerate() {
        let child_is_last = i == node.children.len() - 1;
        render_node(child, &new_prefix, child_is_last);
    }
}
