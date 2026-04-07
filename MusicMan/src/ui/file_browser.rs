use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
    pub children_loaded: bool,
    /// Per-folder toggle: sort by file creation date for track numbering
    pub date_order_enabled: bool,
}

impl FileNode {
    pub fn new(path: PathBuf, depth: usize) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let is_dir = path.is_dir();
        Self {
            path,
            name,
            is_dir,
            depth,
            expanded: false,
            children_loaded: false,
            date_order_enabled: false,
        }
    }
}

/// Actions available on a selected path, shown in the right panel
#[derive(Debug, Clone, PartialEq)]
pub enum FolderAction {
    FetchMetadata,
    CleanMetadata,
    ConvertToM4A,
    DateOrderTracknumbers,
    ExportToIpod,
    WriteBack,
}

impl FolderAction {
    pub fn label(&self) -> &str {
        match self {
            Self::FetchMetadata => "[f] Fetch metadata",
            Self::CleanMetadata => "[c] Clean metadata",
            Self::ConvertToM4A => "[v] Convert to M4A",
            Self::DateOrderTracknumbers => "[d] Date-order track numbers",
            Self::ExportToIpod => "[e] Export to iPod",
            Self::WriteBack => "[w] Write back to source",
        }
    }

    pub fn available_for(path: &PathBuf) -> Vec<Self> {
        if path.is_dir() {
            vec![
                Self::FetchMetadata,
                Self::CleanMetadata,
                Self::ConvertToM4A,
                Self::DateOrderTracknumbers,
                Self::ExportToIpod,
                Self::WriteBack,
            ]
        } else {
            vec![
                Self::FetchMetadata,
                Self::CleanMetadata,
                Self::ConvertToM4A,
            ]
        }
    }
}

pub struct FileBrowser {
    pub root: PathBuf,
    /// Flat list of visible nodes (expanded tree)
    pub nodes: Vec<FileNode>,
    pub selected: usize,
}

impl FileBrowser {
    pub fn new(root: PathBuf) -> Result<Self> {
        let mut browser = Self {
            root: root.clone(),
            nodes: vec![],
            selected: 0,
        };
        browser.load_root()?;
        Ok(browser)
    }

    fn load_root(&mut self) -> Result<()> {
        self.nodes.clear();
        // Load direct children of root
        let entries = Self::read_dir_sorted(&self.root)?;
        for entry in entries {
            self.nodes.push(FileNode::new(entry, 0));
        }
        Ok(())
    }

    fn read_dir_sorted(path: &PathBuf) -> Result<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        // Dirs first, then files, both alphabetical
        entries.sort_by(|a, b| {
            let a_dir = a.is_dir();
            let b_dir = b.is_dir();
            match (a_dir, b_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });
        Ok(entries)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.nodes.len() {
            self.selected += 1;
        }
    }

    pub fn expand_selected(&mut self) {
        let idx = self.selected;
        if idx >= self.nodes.len() {
            return;
        }
        let node = &self.nodes[idx];
        if !node.is_dir || node.expanded {
            return;
        }
        let path = node.path.clone();
        let depth = node.depth;

        let children: Vec<FileNode> = Self::read_dir_sorted(&path)
            .unwrap_or_default()
            .into_iter()
            .map(|p| FileNode::new(p, depth + 1))
            .collect();

        self.nodes[idx].expanded = true;
        self.nodes[idx].children_loaded = true;

        let insert_pos = idx + 1;
        for (i, child) in children.into_iter().enumerate() {
            self.nodes.insert(insert_pos + i, child);
        }
    }

    pub fn collapse_selected(&mut self) {
        let idx = self.selected;
        if idx >= self.nodes.len() {
            return;
        }
        if !self.nodes[idx].is_dir || !self.nodes[idx].expanded {
            // If on a child, collapse parent
            if self.nodes[idx].depth > 0 {
                self.collapse_parent(idx);
            }
            return;
        }
        self.collapse_node(idx);
    }

    fn collapse_node(&mut self, idx: usize) {
        let depth = self.nodes[idx].depth;
        self.nodes[idx].expanded = false;
        // Remove all children (nodes with greater depth immediately after)
        let remove_start = idx + 1;
        let remove_end = self.nodes[remove_start..]
            .iter()
            .position(|n| n.depth <= depth)
            .map(|p| remove_start + p)
            .unwrap_or(self.nodes.len());
        self.nodes.drain(remove_start..remove_end);
    }

    fn collapse_parent(&mut self, idx: usize) {
        let target_depth = self.nodes[idx].depth - 1;
        // Find parent: walk backwards for a dir at depth-1
        if let Some(parent_idx) = (0..idx)
            .rev()
            .find(|&i| self.nodes[i].depth == target_depth && self.nodes[i].is_dir)
        {
            self.selected = parent_idx;
            self.collapse_node(parent_idx);
        }
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.nodes.get(self.selected).map(|n| &n.path)
    }

    pub fn toggle_date_order(&mut self) {
        if let Some(node) = self.nodes.get_mut(self.selected) {
            node.date_order_enabled = !node.date_order_enabled;
        }
    }
}
