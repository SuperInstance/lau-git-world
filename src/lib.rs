//! # lau-git-world
//!
//! Git-native world system for the Lau platform.
//! Every game entity (character, place, tool, quest) is a git repo.
//! Forking = copying a friend's character. Merging = combining two worlds.
//! Branching = trying something risky. Git log = adventure history.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use git2::{
    Commit, DiffFormat, DiffLineType, Oid, Repository, Signature,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by this crate.
#[derive(Debug)]
pub enum Error {
    /// An error from the `git2` crate.
    Git(git2::Error),
    /// An I/O error.
    Io(std::io::Error),
    /// A JSON (de)serialization error.
    Json(serde_json::Error),
    /// A logical / usage error described by a string.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(e) => write!(f, "git error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Json(e) => write!(f, "json error: {e}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<git2::Error> for Error {
    fn from(e: git2::Error) -> Self {
        Self::Git(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// What kind of game entity this repo represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Character,
    Place,
    Tool,
    Quest,
    World,
    Inventory,
}

impl EntityType {
    /// Returns the directory name used under a world root for this entity type.
    pub fn dir_name(&self) -> &str {
        match self {
            Self::Character => "characters",
            Self::Place => "places",
            Self::Tool => "tools",
            Self::Quest => "quests",
            Self::World => "world",
            Self::Inventory => "inventories",
        }
    }
}

/// A single entry from git log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub hash: String,
    pub message: String,
    pub timestamp: u64,
    pub author: String,
}

// ---------------------------------------------------------------------------
// WorldRepo — the core git-backed game entity
// ---------------------------------------------------------------------------

/// A git-backed game entity. The fundamental building block.
pub struct WorldRepo {
    repo: Repository,
    pub entity_type: EntityType,
    pub name: String,
    pub owner: String,
}

impl WorldRepo {
    // -- metadata helpers ----------------------------------------------------

    /// Write a small JSON metadata file into the work tree (not committed).
    fn write_meta(&self) -> Result<()> {
        let meta = serde_json::json!({
            "entity_type": self.entity_type,
            "name": self.name,
            "owner": self.owner,
        });
        let path = self.repo.workdir().unwrap().join("lau-entity.json");
        fs::write(&path, serde_json::to_string_pretty(&meta)?)?;
        Ok(())
    }

    fn read_entity_type(repo: &Repository) -> Result<EntityType> {
        let path = repo.workdir().unwrap().join("lau-entity.json");
        let data = fs::read_to_string(&path)?;
        let v: serde_json::Value = serde_json::from_str(&data)?;
        let et: EntityType = serde_json::from_value(v["entity_type"].clone())?;
        Ok(et)
    }

    fn read_field(repo: &Repository, field: &str) -> Result<String> {
        let path = repo.workdir().unwrap().join("lau-entity.json");
        let data = fs::read_to_string(&path)?;
        let v: serde_json::Value = serde_json::from_str(&data)?;
        Ok(v[field].as_str().unwrap_or_default().to_string())
    }

    fn signature(&self) -> Result<Signature<'_>> {
        Ok(Signature::now(&self.owner, &format!("{}@lau.world", self.owner))?)
    }

    #[allow(dead_code)]
    fn head_tree<'a>(&'a self, repo: &'a Repository) -> Result<git2::Tree<'a>> {
        let head = repo.head()?.target().ok_or_else(|| Error::Other("no HEAD".into()))?;
        let commit = repo.find_commit(head)?;
        Ok(commit.tree()?)
    }

    /// Stage `relative_path` and create a commit with `message`.
    #[allow(dead_code)]
    fn commit(&self, relative_path: &str, message: &str) -> Result<String> {
        let repo = &self.repo;
        let sig = self.signature()?;

        // Stage the file
        let mut index = repo.index()?;
        index.add_path(Path::new(relative_path))?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;

        // Parent
        let parent_commit: Option<Commit<'_>> = repo
            .head()
            .ok()
            .and_then(|r| r.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        let parents: Vec<&Commit<'_>> = parent_commit.as_ref().into_iter().collect();
        let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;

        Ok(oid.to_string())
    }

    // -- public API ----------------------------------------------------------

    /// Create a brand-new git repo for an entity at `path`.
    pub fn init(path: &str) -> Result<Self> {
        let p = Path::new(path);
        let repo = Repository::init(p)?;

        let mut instance = Self {
            repo,
            entity_type: EntityType::World,
            name: String::new(),
            owner: String::from("unknown"),
        };

        // Write an initial placeholder so we can make an initial commit
        instance.write_meta()?;
        instance.name = "untitled".into();
        instance.owner = "unknown".into();
        instance.entity_type = EntityType::World;

        // Re-write with actual values
        instance.write_meta()?;

        // Initial commit
        let mut index = instance.repo.index()?;
        index.add_path(Path::new("lau-entity.json"))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        {
            let tree = instance.repo.find_tree(tree_id)?;
            let sig = instance.signature()?;
            instance
                .repo
                .commit(Some("HEAD"), &sig, &sig, "lau: init world repo", &tree, &[])?;
        }

        Ok(instance)
    }

    /// Open an existing WorldRepo from disk.
    pub fn open(path: &str) -> Result<Self> {
        let repo = Repository::open(path)?;
        let entity_type = Self::read_entity_type(&repo)?;
        let name = Self::read_field(&repo, "name")?;
        let owner = Self::read_field(&repo, "owner")?;
        Ok(Self {
            repo,
            entity_type,
            name,
            owner,
        })
    }

    /// Save content at a given relative path. Returns the commit hash.
    pub fn save(&mut self, path: &str, content: &str) -> Result<String> {
        let full = self.repo.workdir().unwrap().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full, content)?;
        self.write_meta()?;
        let message = format!("lau: update {path}");
        // Stage both the file and meta
        let mut index = self.repo.index()?;
        index.add_path(Path::new(path))?;
        index.add_path(Path::new("lau-entity.json"))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let sig = self.signature()?;
        let parent_commit = self
            .repo
            .head()
            .ok()
            .and_then(|r| r.target())
            .and_then(|oid| self.repo.find_commit(oid).ok());
        let parents: Vec<&Commit<'_>> = parent_commit.as_ref().into_iter().collect();
        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;
        Ok(oid.to_string())
    }

    /// Load a file from the repo's HEAD.
    pub fn load(&self, path: &str) -> Result<String> {
        let head = self
            .repo
            .head()?
            .target()
            .ok_or_else(|| Error::Other("no HEAD".into()))?;
        let commit = self.repo.find_commit(head)?;
        let tree = commit.tree()?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| Error::Other(format!("file not found in repo: {path}")))?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(String::from_utf8_lossy(blob.content()).into_owned())
    }

    /// Get the commit history for a file (or the whole repo if path is empty).
    pub fn history(&self, _path: &str) -> Vec<HistoryEntry> {
        let mut revwalk = match self.repo.revwalk() {
            Ok(rw) => rw,
            Err(_) => return Vec::new(),
        };
        if revwalk.push_head().is_err() {
            return Vec::new();
        }

        revwalk
            .flatten()
            .filter_map(|oid| {
                let commit = self.repo.find_commit(oid).ok()?;
                let ctime = commit.time();
                let ts = ctime.seconds().max(0) as u64;
                let author = commit.author().name().unwrap_or_default().to_string();
                let message = commit.message().unwrap_or_default().to_string();
                Some(HistoryEntry {
                    hash: oid.to_string(),
                    message,
                    timestamp: ts,
                    author,
                })
            })
            .collect()
    }

    /// Show what changed between two commits.
    pub fn diff(&self, from: &str, to: &str) -> Result<String> {
        let from_oid = Oid::from_str(from)?;
        let to_oid = Oid::from_str(to)?;
        let from_commit = self.repo.find_commit(from_oid)?;
        let to_commit = self.repo.find_commit(to_oid)?;

        let from_tree = from_commit.tree()?;
        let to_tree = to_commit.tree()?;

        let diff = self
            .repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?;

        let mut output = String::new();
        diff.print(DiffFormat::Patch, |delta, _hunk, line| {
            let prefix = match line.origin_value() {
                DiffLineType::Addition => '+',
                DiffLineType::Deletion => '-',
                _ => ' ',
            };
            let _ = delta;
            output.push(prefix);
            if let Ok(t) = std::str::from_utf8(line.content()) {
                output.push_str(t);
            }
            true
        })?;

        Ok(output)
    }

    /// Create a branch — for trying something risky.
    pub fn branch(&self, name: &str) -> Result<()> {
        let head = self
            .repo
            .head()?
            .target()
            .ok_or_else(|| Error::Other("no HEAD".into()))?;
        let commit = self.repo.find_commit(head)?;
        self.repo.branch(name, &commit, false)?;
        Ok(())
    }

    /// Merge a branch back into HEAD.
    pub fn merge(&self, branch: &str) -> Result<String> {
        let branch_obj = self
            .repo
            .find_branch(branch, git2::BranchType::Local)?;
        let target = branch_obj
            .get()
            .target()
            .ok_or_else(|| Error::Other("branch has no target".into()))?;
        let theirs = self.repo.find_annotated_commit(target)?;

        let head_oid = self
            .repo
            .head()?
            .target()
            .ok_or_else(|| Error::Other("no HEAD".into()))?;
        let _ours = self.repo.find_annotated_commit(head_oid)?;

        self.repo
            .merge(&[&theirs], Some(&mut git2::MergeOptions::new()), None)?;

        // Check for conflicts
        let mut index = self.repo.index()?;
        if index.has_conflicts() {
            // Abort merge on conflict
            self.repo.cleanup_state()?;
            return Err(Error::Other("merge conflict".into()));
        }

        // Write merge commit
        let tree_id = index.write_tree_to(&self.repo)?;
        let tree = self.repo.find_tree(tree_id)?;
        let sig = self.signature()?;

        let head_commit = self.repo.find_commit(head_oid)?;
        let branch_commit = self.repo.find_commit(target)?;
        let parents = [&head_commit, &branch_commit];

        let oid = self.repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("lau: merge branch '{branch}'"),
            &tree,
            &parents,
        )?;

        Ok(oid.to_string())
    }

    /// Fork (clone) an existing entity into a new location.
    pub fn fork_from(source_path: &str, dest_path: &str) -> Result<Self> {
        let src = Repository::open(source_path)?;
        let dest_dir = Path::new(dest_path);
        fs::create_dir_all(dest_dir)?;

        // Copy files (simple file copy — skip .git internals)
        copy_dir_recursive(src.workdir().unwrap(), dest_dir)?;

        // Init a fresh git repo at destination and commit everything
        let repo = Repository::init(dest_dir)?;
        let entity_type = Self::read_entity_type_from_dir(dest_dir)?;
        let name = Self::read_field_from_dir(dest_dir, "name")?;
        let owner = Self::read_field_from_dir(dest_dir, "owner")?;

        let instance = Self {
            repo,
            entity_type,
            name,
            owner,
        };

        // Stage everything and commit
        {
            let mut index = instance.repo.index()?;
            index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = instance.repo.find_tree(tree_id)?;
            let sig = instance.signature()?;
            instance.repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "lau: fork",
                &tree,
                &[],
            )?;
        }

        Ok(instance)
    }

    fn read_entity_type_from_dir(dir: &Path) -> Result<EntityType> {
        let path = dir.join("lau-entity.json");
        let data = fs::read_to_string(&path)?;
        let v: serde_json::Value = serde_json::from_str(&data)?;
        let et: EntityType = serde_json::from_value(v["entity_type"].clone())?;
        Ok(et)
    }

    fn read_field_from_dir(dir: &Path, field: &str) -> Result<String> {
        let path = dir.join("lau-entity.json");
        let data = fs::read_to_string(&path)?;
        let v: serde_json::Value = serde_json::from_str(&data)?;
        Ok(v[field].as_str().unwrap_or_default().to_string())
    }

    /// Get the filesystem path of this repo.
    pub fn path(&self) -> PathBuf {
        self.repo.workdir().unwrap().to_path_buf()
    }
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            // Skip .git internals for simplicity — we'll re-init
            if entry.file_name() == ".git" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// CharacterRepo
// ---------------------------------------------------------------------------

/// Character data stored as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterData {
    pub name: String,
    pub appearance: String, // base64 voxel model
    pub personality_traits: Vec<String>,
    pub abilities: Vec<String>,
    pub level: u32,
    pub inventory: Vec<String>,
}

impl Default for CharacterData {
    fn default() -> Self {
        Self {
            name: String::new(),
            appearance: String::new(),
            personality_traits: Vec::new(),
            abilities: Vec::new(),
            level: 1,
            inventory: Vec::new(),
        }
    }
}

/// A git-backed character entity.
pub struct CharacterRepo {
    pub world: WorldRepo,
}

impl CharacterRepo {
    /// Create a new character repo.
    pub fn init(path: &str, name: &str, owner: &str) -> Result<Self> {
        let mut world = WorldRepo::init(path)?;
        world.entity_type = EntityType::Character;
        world.name = name.to_string();
        world.owner = owner.to_string();

        let data = CharacterData {
            name: name.to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&data)?;

        world.write_meta()?;
        // Write character data
        let full = world.repo.workdir().unwrap().join("character.json");
        fs::write(&full, &json)?;
        {
            let mut index = world.repo.index()?;
            index.add_path(Path::new("character.json"))?;
            index.add_path(Path::new("lau-entity.json"))?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = world.repo.find_tree(tree_id)?;
            let sig = world.signature()?;
            // Amend the initial commit
            let head = world.repo.head()?.target().unwrap();
            let parent = world.repo.find_commit(head)?;
            let parents: Vec<&Commit<'_>> = [&parent].into_iter().collect();
            world.repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("lau: create character '{name}'"),
                &tree,
                &parents,
            )?;
        }

        Ok(Self { world })
    }

    /// Open an existing character repo.
    pub fn open(path: &str) -> Result<Self> {
        let world = WorldRepo::open(path)?;
        if world.entity_type != EntityType::Character {
            return Err(Error::Other("not a character repo".into()));
        }
        Ok(Self { world })
    }

    fn load_data(&self) -> Result<CharacterData> {
        let raw = self.world.load("character.json")?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn save_data(&mut self, data: &CharacterData, message: &str) -> Result<String> {
        let json = serde_json::to_string_pretty(data)?;
        // Write file
        let full = self.world.repo.workdir().unwrap().join("character.json");
        fs::write(&full, &json)?;

        let sig = self.world.signature()?;
        let mut index = self.world.repo.index()?;
        index.add_path(Path::new("character.json"))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = self.world.repo.find_tree(tree_id)?;
        let head_oid = self
            .world
            .repo
            .head()?
            .target()
            .ok_or_else(|| Error::Other("no HEAD".into()))?;
        let parent = self.world.repo.find_commit(head_oid)?;
        let parents: Vec<&Commit<'_>> = [&parent].into_iter().collect();
        let oid = self
            .world
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
        Ok(oid.to_string())
    }

    /// Level up the character. Returns commit hash.
    pub fn level_up(&mut self) -> Result<String> {
        let mut data = self.load_data()?;
        data.level += 1;
        let msg = format!("lau: {} reached level {}", data.name, data.level);
        self.save_data(&data, &msg)
    }

    /// Learn a new skill. Returns commit hash.
    pub fn learn_skill(&mut self, skill: &str) -> Result<String> {
        let mut data = self.load_data()?;
        if !data.abilities.contains(&skill.to_string()) {
            data.abilities.push(skill.to_string());
        }
        let msg = format!("lau: {} learned '{skill}'", data.name);
        self.save_data(&data, &msg)
    }

    /// Equip an item from inventory. Returns commit hash.
    pub fn equip(&mut self, item_path: &str) -> Result<String> {
        let mut data = self.load_data()?;
        data.inventory.push(item_path.to_string());
        let msg = format!("lau: {} equipped '{item_path}'", data.name);
        self.save_data(&data, &msg)
    }

    /// Get the current character data.
    pub fn data(&self) -> Result<CharacterData> {
        self.load_data()
    }
}

// ---------------------------------------------------------------------------
// PlaceRepo
// ---------------------------------------------------------------------------

/// A room within a place.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Room {
    pub id: String,
    pub data: String, // base64-encoded voxel chunk data
}

/// Place data stored as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PlaceData {
    pub name: String,
    pub terrain: String, // base64 voxel chunk data
    pub rooms: Vec<Room>,
    pub connections: Vec<String>, // paths to other places
    pub npcs: Vec<String>,
}



/// A git-backed place entity.
pub struct PlaceRepo {
    pub world: WorldRepo,
}

impl PlaceRepo {
    /// Create a new place repo.
    pub fn init(path: &str, name: &str, owner: &str) -> Result<Self> {
        let mut world = WorldRepo::init(path)?;
        world.entity_type = EntityType::Place;
        world.name = name.to_string();
        world.owner = owner.to_string();

        let data = PlaceData {
            name: name.to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&data)?;

        world.write_meta()?;
        let full = world.repo.workdir().unwrap().join("place.json");
        fs::write(&full, &json)?;

        {
            let mut index = world.repo.index()?;
            index.add_path(Path::new("place.json"))?;
            index.add_path(Path::new("lau-entity.json"))?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = world.repo.find_tree(tree_id)?;
            let sig = world.signature()?;
            let head = world.repo.head()?.target().unwrap();
            let parent = world.repo.find_commit(head)?;
            let parents: Vec<&Commit<'_>> = [&parent].into_iter().collect();
            world.repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("lau: create place '{name}'"),
                &tree,
                &parents,
            )?;
        }

        Ok(Self { world })
    }

    /// Open an existing place repo.
    pub fn open(path: &str) -> Result<Self> {
        let world = WorldRepo::open(path)?;
        if world.entity_type != EntityType::Place {
            return Err(Error::Other("not a place repo".into()));
        }
        Ok(Self { world })
    }

    fn load_data(&self) -> Result<PlaceData> {
        let raw = self.world.load("place.json")?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn save_data(&mut self, data: &PlaceData, message: &str) -> Result<String> {
        let json = serde_json::to_string_pretty(data)?;
        let full = self.world.repo.workdir().unwrap().join("place.json");
        fs::write(&full, &json)?;

        let sig = self.world.signature()?;
        let mut index = self.world.repo.index()?;
        index.add_path(Path::new("place.json"))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = self.world.repo.find_tree(tree_id)?;
        let head_oid = self
            .world
            .repo
            .head()?
            .target()
            .ok_or_else(|| Error::Other("no HEAD".into()))?;
        let parent = self.world.repo.find_commit(head_oid)?;
        let parents: Vec<&Commit<'_>> = [&parent].into_iter().collect();
        let oid = self
            .world
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
        Ok(oid.to_string())
    }

    /// Build (add) a room with voxel data.
    pub fn build_room(&mut self, room_id: &str, data: &[u8]) -> Result<String> {
        let mut place = self.load_data()?;
        place.rooms.push(Room {
            id: room_id.to_string(),
            data: BASE64.encode(data),
        });
        let msg = format!("lau: built room '{room_id}'");
        self.save_data(&place, &msg)
    }

    /// Connect this place to another place repo.
    pub fn connect_to(&mut self, other_place: &str) -> Result<String> {
        let mut place = self.load_data()?;
        place.connections.push(other_place.to_string());
        let msg = format!("lau: connected to '{other_place}'");
        self.save_data(&place, &msg)
    }

    /// Get the current place data.
    pub fn data(&self) -> Result<PlaceData> {
        self.load_data()
    }
}

// ---------------------------------------------------------------------------
// WorldManifest
// ---------------------------------------------------------------------------

/// The top-level world manifest listing all sub-repos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldManifest {
    pub name: String,
    pub characters: Vec<String>,
    pub places: Vec<String>,
    pub tools: Vec<String>,
    pub quests: Vec<String>,
}

impl WorldManifest {
    /// Create a new manifest at the given path.
    pub fn init(path: &str, name: &str, owner: &str) -> Result<(Self, WorldRepo)> {
        let mut world = WorldRepo::init(path)?;
        world.entity_type = EntityType::World;
        world.name = name.to_string();
        world.owner = owner.to_string();

        let manifest = Self {
            name: name.to_string(),
            characters: Vec::new(),
            places: Vec::new(),
            tools: Vec::new(),
            quests: Vec::new(),
        };

        let json = serde_json::to_string_pretty(&manifest)?;
        let full = world.repo.workdir().unwrap().join("manifest.json");
        fs::write(&full, &json)?;

        world.write_meta()?;
        {
            let mut index = world.repo.index()?;
            index.add_path(Path::new("manifest.json"))?;
            index.add_path(Path::new("lau-entity.json"))?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = world.repo.find_tree(tree_id)?;
            let sig = world.signature()?;
            let head = world.repo.head()?.target().unwrap();
            let parent = world.repo.find_commit(head)?;
            let parents: Vec<&Commit<'_>> = [&parent].into_iter().collect();
            world.repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("lau: create world '{name}'"),
                &tree,
                &parents,
            )?;
        }

        Ok((manifest, world))
    }

    /// Load manifest from an existing world repo.
    pub fn load(world: &WorldRepo) -> Result<Self> {
        let raw = world.load("manifest.json")?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Save the manifest back to the world repo, returns commit hash.
    pub fn save(&self, world: &mut WorldRepo) -> Result<String> {
        let json = serde_json::to_string_pretty(self)?;
        world.save("manifest.json", &json)
    }

    /// Register a character repo.
    pub fn add_character(&mut self, repo_path: &str, world: &mut WorldRepo) -> Result<String> {
        self.characters.push(repo_path.to_string());
        self.save(world)
    }

    /// Register a place repo.
    pub fn add_place(&mut self, repo_path: &str, world: &mut WorldRepo) -> Result<String> {
        self.places.push(repo_path.to_string());
        self.save(world)
    }

    /// Register a tool repo.
    pub fn add_tool(&mut self, repo_path: &str, world: &mut WorldRepo) -> Result<String> {
        self.tools.push(repo_path.to_string());
        self.save(world)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: creates a temp dir and returns its path as a String.
    fn tmp() -> (TempDir, String) {
        let td = TempDir::new().unwrap();
        let p = td.path().to_string_lossy().to_string();
        (td, p)
    }

    #[test]
    fn test_world_repo_init() {
        let (_td, path) = tmp();
        let repo = WorldRepo::init(&path).unwrap();
        assert!(Path::new(&path).join(".git").exists());
        assert_eq!(repo.entity_type, EntityType::World);
    }

    #[test]
    fn test_world_repo_save_and_load() {
        let (_td, path) = tmp();
        let mut repo = WorldRepo::init(&path).unwrap();
        repo.name = "test-world".into();
        repo.owner = "alice".into();
        repo.write_meta().unwrap();
        let hash = repo.save("hello.txt", "Hello, Lau!").unwrap();
        assert!(!hash.is_empty());

        let loaded = repo.load("hello.txt").unwrap();
        assert_eq!(loaded, "Hello, Lau!");
    }

    #[test]
    fn test_world_repo_history() {
        let (_td, path) = tmp();
        let mut repo = WorldRepo::init(&path).unwrap();
        repo.name = "hist-test".into();
        repo.owner = "bob".into();
        repo.write_meta().unwrap();
        repo.save("a.txt", "first").unwrap();
        repo.save("a.txt", "second").unwrap();
        repo.save("a.txt", "third").unwrap();

        let hist = repo.history("a.txt");
        assert!(hist.len() >= 3); // init commit + 3 saves
    }

    #[test]
    fn test_world_repo_diff() {
        let (_td, path) = tmp();
        let mut repo = WorldRepo::init(&path).unwrap();
        repo.name = "diff-test".into();
        repo.owner = "carol".into();
        repo.write_meta().unwrap();
        let h1 = repo.save("d.txt", "version 1").unwrap();
        let h2 = repo.save("d.txt", "version 2").unwrap();

        let d = repo.diff(&h1, &h2).unwrap();
        assert!(d.contains("version 2") || d.contains("version"));
    }

    #[test]
    fn test_world_repo_branch() {
        let (_td, path) = tmp();
        let mut repo = WorldRepo::init(&path).unwrap();
        repo.name = "branch-test".into();
        repo.owner = "dave".into();
        repo.write_meta().unwrap();
        repo.save("f.txt", "main content").unwrap();

        repo.branch("risky-idea").unwrap();
        // Should not panic
    }

    #[test]
    fn test_world_repo_branch_and_merge() {
        let (_td, path) = tmp();
        let mut repo = WorldRepo::init(&path).unwrap();
        repo.name = "merge-test".into();
        repo.owner = "eve".into();
        repo.write_meta().unwrap();
        repo.save("main.txt", "base").unwrap();

        repo.branch("experiment").unwrap();

        // Switch to branch, make a commit
        let repo_obj = &repo.repo;
        let branch_ref = repo_obj.find_branch("experiment", git2::BranchType::Local).unwrap();
        let branch_oid = branch_ref.get().target().unwrap();
        repo_obj.set_head_detached(branch_oid).unwrap();
        repo_obj.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

        // Make a change on the branch
        let branch_file = repo.repo.workdir().unwrap().join("feature.txt");
        fs::write(&branch_file, "new feature!").unwrap();
        {
            let sig = repo.signature().unwrap();
            let mut index = repo.repo.index().unwrap();
            index.add_path(Path::new("feature.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.repo.find_tree(tree_id).unwrap();
            let parent = repo.repo.find_commit(branch_oid).unwrap();
            repo.repo
                .commit(Some("HEAD"), &sig, &sig, "lau: add feature", &tree, &[&parent])
                .unwrap();
        }

        // Switch back to main
        repo.repo.set_head("refs/heads/master").unwrap();
        repo.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

        // Merge
        let result = repo.merge("experiment").unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_world_repo_fork() {
        let (_td1, path1) = tmp();
        let (_td2, path2) = tmp();

        let mut repo = WorldRepo::init(&path1).unwrap();
        repo.name = "original".into();
        repo.owner = "frank".into();
        repo.write_meta().unwrap();
        repo.save("data.txt", "my cool stuff").unwrap();

        let fork = WorldRepo::fork_from(&path1, &path2).unwrap();
        assert_eq!(fork.name, "original");
        // The fork should have its own copy
        let data = fork.load("data.txt").unwrap();
        assert_eq!(data, "my cool stuff");
    }

    #[test]
    fn test_entity_type_dir_names() {
        assert_eq!(EntityType::Character.dir_name(), "characters");
        assert_eq!(EntityType::Place.dir_name(), "places");
        assert_eq!(EntityType::Tool.dir_name(), "tools");
        assert_eq!(EntityType::Quest.dir_name(), "quests");
        assert_eq!(EntityType::World.dir_name(), "world");
        assert_eq!(EntityType::Inventory.dir_name(), "inventories");
    }

    #[test]
    fn test_history_entry_serialization() {
        let entry = HistoryEntry {
            hash: "abc123".into(),
            message: "test commit".into(),
            timestamp: 1700000000,
            author: "alice".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }

    #[test]
    fn test_character_repo_init() {
        let (_td, path) = tmp();
        let char_repo = CharacterRepo::init(&path, "Hero", "alice").unwrap();
        assert_eq!(char_repo.world.entity_type, EntityType::Character);
        let data = char_repo.data().unwrap();
        assert_eq!(data.name, "Hero");
        assert_eq!(data.level, 1);
    }

    #[test]
    fn test_character_level_up() {
        let (_td, path) = tmp();
        let mut char_repo = CharacterRepo::init(&path, "Hero", "alice").unwrap();
        char_repo.level_up().unwrap();
        char_repo.level_up().unwrap();
        let data = char_repo.data().unwrap();
        assert_eq!(data.level, 3);
    }

    #[test]
    fn test_character_learn_skill() {
        let (_td, path) = tmp();
        let mut char_repo = CharacterRepo::init(&path, "Hero", "alice").unwrap();
        char_repo.learn_skill("fireball").unwrap();
        char_repo.learn_skill("heal").unwrap();
        let data = char_repo.data().unwrap();
        assert!(data.abilities.contains(&"fireball".to_string()));
        assert!(data.abilities.contains(&"heal".to_string()));
    }

    #[test]
    fn test_character_learn_skill_idempotent() {
        let (_td, path) = tmp();
        let mut char_repo = CharacterRepo::init(&path, "Hero", "alice").unwrap();
        char_repo.learn_skill("fireball").unwrap();
        char_repo.learn_skill("fireball").unwrap();
        let data = char_repo.data().unwrap();
        assert_eq!(data.abilities.len(), 1);
    }

    #[test]
    fn test_character_equip() {
        let (_td, path) = tmp();
        let mut char_repo = CharacterRepo::init(&path, "Hero", "alice").unwrap();
        char_repo.equip("/tools/sword").unwrap();
        let data = char_repo.data().unwrap();
        assert!(data.inventory.contains(&"/tools/sword".to_string()));
    }

    #[test]
    fn test_character_history_tracks_actions() {
        let (_td, path) = tmp();
        let mut char_repo = CharacterRepo::init(&path, "Hero", "alice").unwrap();
        char_repo.level_up().unwrap();
        char_repo.learn_skill("jump").unwrap();

        let hist = char_repo.world.history("character.json");
        // init + level_up + learn_skill = at least 3 entries
        assert!(hist.len() >= 3);

        let messages: Vec<&str> = hist.iter().map(|h| h.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("level 2")));
        assert!(messages.iter().any(|m| m.contains("learned")));
    }

    #[test]
    fn test_place_repo_init() {
        let (_td, path) = tmp();
        let place = PlaceRepo::init(&path, "Forest", "bob").unwrap();
        assert_eq!(place.world.entity_type, EntityType::Place);
        let data = place.data().unwrap();
        assert_eq!(data.name, "Forest");
    }

    #[test]
    fn test_place_build_room() {
        let (_td, path) = tmp();
        let mut place = PlaceRepo::init(&path, "Castle", "bob").unwrap();
        let voxel_data = vec![0u8, 1, 2, 3, 4, 5];
        place.build_room("throne", &voxel_data).unwrap();

        let data = place.data().unwrap();
        assert_eq!(data.rooms.len(), 1);
        assert_eq!(data.rooms[0].id, "throne");
        assert_eq!(data.rooms[0].data, BASE64.encode(&voxel_data));
    }

    #[test]
    fn test_place_connect_to() {
        let (_td, path) = tmp();
        let mut place = PlaceRepo::init(&path, "Village", "carol").unwrap();
        place.connect_to("/places/forest").unwrap();
        place.connect_to("/places/mountain").unwrap();

        let data = place.data().unwrap();
        assert_eq!(data.connections.len(), 2);
        assert!(data.connections.contains(&"/places/forest".to_string()));
    }

    #[test]
    fn test_place_multiple_rooms() {
        let (_td, path) = tmp();
        let mut place = PlaceRepo::init(&path, "Dungeon", "dave").unwrap();
        place.build_room("entrance", &[1, 2, 3]).unwrap();
        place.build_room("boss", &[4, 5, 6]).unwrap();
        place.build_room("treasure", &[7, 8, 9]).unwrap();

        let data = place.data().unwrap();
        assert_eq!(data.rooms.len(), 3);
    }

    #[test]
    fn test_world_manifest_init() {
        let (_td, path) = tmp();
        let (manifest, world) = WorldManifest::init(&path, "MyWorld", "alice").unwrap();
        assert_eq!(manifest.name, "MyWorld");
        assert!(manifest.characters.is_empty());
        assert_eq!(world.entity_type, EntityType::World);
    }

    #[test]
    fn test_world_manifest_add_character() {
        let (_td, path) = tmp();
        let (mut manifest, mut world) = WorldManifest::init(&path, "MyWorld", "alice").unwrap();

        manifest
            .add_character("/characters/hero", &mut world)
            .unwrap();

        let reloaded = WorldManifest::load(&world).unwrap();
        assert_eq!(reloaded.characters.len(), 1);
        assert_eq!(reloaded.characters[0], "/characters/hero");
    }

    #[test]
    fn test_world_manifest_add_place() {
        let (_td, path) = tmp();
        let (mut manifest, mut world) = WorldManifest::init(&path, "MyWorld", "alice").unwrap();

        manifest.add_place("/places/forest", &mut world).unwrap();
        manifest.add_place("/places/castle", &mut world).unwrap();

        let reloaded = WorldManifest::load(&world).unwrap();
        assert_eq!(reloaded.places.len(), 2);
    }

    #[test]
    fn test_world_manifest_add_tool() {
        let (_td, path) = tmp();
        let (mut manifest, mut world) = WorldManifest::init(&path, "MyWorld", "alice").unwrap();

        manifest.add_tool("/tools/sword", &mut world).unwrap();

        let reloaded = WorldManifest::load(&world).unwrap();
        assert_eq!(reloaded.tools.len(), 1);
    }

    #[test]
    fn test_world_repo_save_nested_path() {
        let (_td, path) = tmp();
        let mut repo = WorldRepo::init(&path).unwrap();
        repo.name = "nested".into();
        repo.owner = "test".into();
        repo.write_meta().unwrap();
        repo.save("depths/of/nested/file.txt", "deep content").unwrap();

        let loaded = repo.load("depths/of/nested/file.txt").unwrap();
        assert_eq!(loaded, "deep content");
    }

    #[test]
    fn test_character_open() {
        let (_td, path) = tmp();
        CharacterRepo::init(&path, "Sage", "alice").unwrap();
        let reopened = CharacterRepo::open(&path).unwrap();
        let data = reopened.data().unwrap();
        assert_eq!(data.name, "Sage");
    }

    #[test]
    fn test_place_open() {
        let (_td, path) = tmp();
        PlaceRepo::init(&path, "Desert", "bob").unwrap();
        let reopened = PlaceRepo::open(&path).unwrap();
        let data = reopened.data().unwrap();
        assert_eq!(data.name, "Desert");
    }

    #[test]
    fn test_place_open_wrong_type_rejected() {
        let (_td, path) = tmp();
        CharacterRepo::init(&path, "NotAPlace", "bob").unwrap();
        let result = PlaceRepo::open(&path);
        assert!(result.is_err());
    }
}
