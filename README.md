# lau-git-world

> Kids' game worlds ARE git repos. Fork a friend's character. Branch to try something risky. Merge two worlds together.

## What This Does

This is the core world system for the **Lau (Layered Agent-UI)** platform — a gamified learning platform where young users explore mathematics through a voxel game world. The radical idea: every game entity (character, place, tool, quest) is a git repository. Git operations become game mechanics.

**Forking** = copying a friend's character. **Branching** = trying something risky without consequences. **Merging** = combining two players' worlds. **Git log** = adventure history. **Diff** = seeing what changed after a quest.

## The Key Idea

Version control is one of the most powerful ideas in computing, but it's locked behind intimidating CLI tools. Lau makes git tangible and playful. When a 10-year-old forks their friend's castle and adds a tower, they're learning collaborative development without knowing it. When they branch before exploring a dangerous cave, they're learning safe experimentation. When they merge their changes back, they're learning conflict resolution.

The entire game history lives in git. No database. No server. Just repos.

## Install

```bash
cargo add lau-git-world
```

## Quick Start

### Create a World

```rust
use lau_git_world::WorldRepo;

// Create a new game world (it's a git repo)
let world = WorldRepo::init("/tmp/my-adventure")?;

// Save a game entity into the world
world.save_entity("characters/hero", &hero_data)?;

// Commit the change with a message
world.commit("Created my hero!")?;
```

### Fork a Friend's World

```rust
// Fork = clone their world into yours
let my_world = WorldRepo::fork(
    "https://github.com/friend/their-world",
    "/tmp/my-copy"
)?;

// Now it's yours. Modify freely.
my_world.save_entity("characters/hero", &my_modified_hero)?;
my_world.commit("Gave hero a jetpack")?;
```

### Branch for Risky Exploration

```rust
// Create a branch before entering the volcano
world.create_branch("volcano-expedition")?;
world.checkout("volcano-expedition")?;

// Explore freely — if things go wrong, just checkout main
world.save_entity("hero", &damaged_hero)?;
world.commit("Fell into lava...")?;

// Oops! Revert to safe state
world.checkout("main")?;
// Hero is fine again. The bad timeline still exists in the branch.
```

### View Adventure History

```rust
// Git log = adventure diary
let history = world.log()?;
for entry in &history {
    println!("{}: {}", entry.timestamp, entry.message);
    // "2026-06-01: Built a bridge across the canyon"
    // "2026-06-01: Discovered the Crystal Caves biome"
    // "2026-05-31: Created my hero!"
}
```

### Merge Two Worlds

```rust
// Merge your changes with a friend's
world.merge("friend-branch")?;

// If there are conflicts (both edited the same entity)
// the game presents them as "world collisions" to resolve
let conflicts = world.unresolved_conflicts()?;
for c in &conflicts {
    println!("Conflict on {}: yours vs theirs", c.entity_path);
}
```

## API Reference

### WorldRepo

| Method | Description |
|--------|-------------|
| `WorldRepo::init(path)` | Create new world (git init) |
| `WorldRepo::open(path)` | Open existing world |
| `WorldRepo::fork(url, path)` | Clone someone else's world |
| `repo.save_entity(path, data)` | Save game entity (git add + write) |
| `repo.load_entity(path)` | Load game entity |
| `repo.commit(message)` | Commit changes |
| `repo.create_branch(name)` | Create a branch |
| `repo.checkout(branch)` | Switch to branch |
| `repo.merge(branch)` | Merge branch into current |
| `repo.log()` | Get commit history |
| `repo.diff(from, to)` | See changes between states |
| `repo.list_entities()` | All entities in world |
| `repo.delete_entity(path)` | Remove entity |

### Entity

| Method | Description |
|--------|-------------|
| `Entity::new(name, kind)` | Create game entity |
| `entity.set_property(key, value)` | Set a property |
| `entity.get_property(key)` | Get a property |
| `entity.to_json()` | Serialize |
| `Entity::from_json(data)` | Deserialize |

## How It Works

Under the hood, every world operation maps to a git operation:
- `save_entity` → write file + `git add`
- `commit` → `git commit`
- `create_branch` → `git branch`
- `checkout` → `git checkout`
- `merge` → `git merge`
- `fork` → `git clone`
- `log` → `git log --format=json`
- `diff` → `git diff`

Entity data is stored as JSON files in a directory structure. The git history IS the game history. No additional database needed.

Uses the `git2` crate (libgit2 bindings) for all git operations.

## The Educational Vision

Lau teaches these concepts through gameplay:

| Game Action | Git Concept | Real-World Skill |
|------------|-------------|-----------------|
| Copy friend's world | Fork | Open source collaboration |
| Try risky exploration | Branch | Safe experimentation |
| Keep your changes | Commit | Version tracking |
| Combine worlds | Merge | Conflict resolution |
| View adventure log | Git log | Audit trails |
| See what changed | Diff | Code review |
| Share your world | Push | Publishing |

## Testing

27 tests covering: world creation, entity CRUD, branching, merging, conflict detection, history logging, serialization, forking.

## Part of the Lau Platform

- **lau-git-world** — You are here. Git-native game worlds.
- **lau-quest** — Quest/mission system for learning
- **lau-biome** — 10 distinct ecological zones
- **lau-spatial** — QuadTree, GridHash, SpatialHash
- **lau-audio** — Procedural audio from math
- **lau-scheduler** — Tick-based game loop
- **lau-memory-arena** — Custom allocator for game entities
- **lau-genealogy** — Lineage tracking for ideas and agents
- **lau-recipe** — Crafting recipe system

## License

MIT
