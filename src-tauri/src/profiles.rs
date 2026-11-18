//! Profile records in the application database.
//!
//! Rules from the specification:
//! - There is always exactly one main profile.
//! - The main profile cannot be deleted until another profile is designated.
//! - Deleted profiles stay recoverable for 90 days, then are purged.
//! - Nothing here ever touches the user's music files.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{AppError, Result};

pub const RETENTION_DAYS: i64 = 90;
const SECONDS_PER_DAY: i64 = 86_400;
const MAX_NAME_CHARS: usize = 40;
const ACTIVE_KEY: &str = "active_profile_id";
const DEFAULT_NAME: &str = "Main";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub is_main: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeletedProfile {
    pub id: String,
    pub name: String,
    pub deleted_at: i64,
    pub days_remaining: i64,
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("Enter a profile name."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::invalid(format!(
            "Profile names can be at most {MAX_NAME_CHARS} characters."
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::invalid(
            "Profile names cannot contain control characters.",
        ));
    }
    Ok(name.to_string())
}

fn name_taken(conn: &Connection, name: &str, except_id: Option<&str>) -> Result<bool> {
    let lowered = name.to_lowercase();
    Ok(list(conn)?
        .iter()
        .any(|p| p.name.to_lowercase() == lowered && Some(p.id.as_str()) != except_id))
}

pub fn list(conn: &Connection) -> Result<Vec<Profile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, is_main, created_at FROM profiles
         WHERE deleted_at IS NULL ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Profile {
            id: row.get(0)?,
            name: row.get(1)?,
            is_main: row.get::<_, i64>(2)? != 0,
            created_at: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn list_deleted(conn: &Connection, now: i64) -> Result<Vec<DeletedProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, deleted_at FROM profiles
         WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let deleted_at: i64 = row.get(2)?;
        let elapsed_days = (now - deleted_at).max(0) / SECONDS_PER_DAY;
        Ok(DeletedProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            deleted_at,
            days_remaining: (RETENTION_DAYS - elapsed_days).max(0),
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn get(conn: &Connection, id: &str) -> Result<Profile> {
    list(conn)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound("That profile no longer exists.".into()))
}

fn insert(conn: &Connection, name: &str, is_main: bool, now: i64) -> Result<Profile> {
    let profile = Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        is_main,
        created_at: now,
    };
    conn.execute(
        "INSERT INTO profiles (id, name, is_main, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![profile.id, profile.name, is_main as i64, now],
    )?;
    Ok(profile)
}

pub fn create(conn: &Connection, name: &str, now: i64) -> Result<Profile> {
    let name = clean_name(name)?;
    if name_taken(conn, &name, None)? {
        return Err(AppError::invalid(
            "A profile with that name already exists.",
        ));
    }
    insert(conn, &name, false, now)
}

pub fn rename(conn: &Connection, id: &str, name: &str) -> Result<Profile> {
    let name = clean_name(name)?;
    get(conn, id)?;
    if name_taken(conn, &name, Some(id))? {
        return Err(AppError::invalid(
            "A profile with that name already exists.",
        ));
    }
    conn.execute(
        "UPDATE profiles SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    get(conn, id)
}

pub fn active_id(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [ACTIVE_KEY],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn set_active(conn: &Connection, id: &str) -> Result<()> {
    get(conn, id)?;
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![ACTIVE_KEY, id],
    )?;
    Ok(())
}

/// Guarantees a main profile and a valid active profile exist.
/// Returns the active profile id.
pub fn ensure_default(conn: &Connection, now: i64) -> Result<String> {
    let tx = conn.unchecked_transaction()?;
    let mut profiles = list(&tx)?;
    if profiles.is_empty() {
        profiles.push(insert(&tx, DEFAULT_NAME, true, now)?);
    } else if !profiles.iter().any(|p| p.is_main) {
        // Should not happen, but never leave the app without a main profile.
        let first = profiles[0].id.clone();
        tx.execute("UPDATE profiles SET is_main = 1 WHERE id = ?1", [&first])?;
        profiles[0].is_main = true;
    }
    let active = match active_id(&tx)? {
        Some(id) if profiles.iter().any(|p| p.id == id) => id,
        _ => {
            let main = profiles
                .iter()
                .find(|p| p.is_main)
                .map(|p| p.id.clone())
                .unwrap_or_else(|| profiles[0].id.clone());
            set_active(&tx, &main)?;
            main
        }
    };
    tx.commit()?;
    Ok(active)
}

pub fn set_main(conn: &Connection, id: &str) -> Result<()> {
    get(conn, id)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE profiles SET is_main = 0 WHERE is_main = 1", [])?;
    tx.execute("UPDATE profiles SET is_main = 1 WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

/// Moves a profile to the recycle bin. Returns the id that is active afterwards.
pub fn delete(conn: &Connection, id: &str, now: i64) -> Result<String> {
    let profile = get(conn, id)?;
    if profile.is_main {
        return Err(AppError::invalid(
            "The main profile cannot be deleted. Make another profile the main profile first.",
        ));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE profiles SET deleted_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    let mut active = active_id(&tx)?.unwrap_or_default();
    if active == id || active.is_empty() {
        active = list(&tx)?
            .into_iter()
            .find(|p| p.is_main)
            .map(|p| p.id)
            .ok_or_else(|| AppError::invalid("No main profile is available."))?;
        set_active(&tx, &active)?;
    }
    tx.commit()?;
    Ok(active)
}

pub fn restore(conn: &Connection, id: &str) -> Result<Profile> {
    let name: Option<String> = conn
        .query_row(
            "SELECT name FROM profiles WHERE id = ?1 AND deleted_at IS NOT NULL",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(mut name) = name else {
        return Err(AppError::NotFound(
            "That profile is no longer in the recycle bin.".into(),
        ));
    };
    let mut attempt = 1;
    let base = name.clone();
    while name_taken(conn, &name, None)? {
        attempt += 1;
        name = format!("{base} ({attempt})");
    }
    conn.execute(
        "UPDATE profiles SET deleted_at = NULL, name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    get(conn, id)
}

/// Permanently removes one profile that is already in the recycle bin.
/// The caller removes its internal data folder. Music files are not involved.
pub fn purge(conn: &Connection, id: &str) -> Result<()> {
    let changed = conn.execute(
        "DELETE FROM profiles WHERE id = ?1 AND deleted_at IS NOT NULL",
        [id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(
            "That profile is no longer in the recycle bin.".into(),
        ));
    }
    Ok(())
}

/// Permanently removes profiles deleted more than 90 days ago.
/// Returns their ids so the caller can remove the internal data folders.
pub fn purge_expired(conn: &Connection, now: i64) -> Result<Vec<String>> {
    let cutoff = now - RETENTION_DAYS * SECONDS_PER_DAY;
    let mut stmt =
        conn.prepare("SELECT id FROM profiles WHERE deleted_at IS NOT NULL AND deleted_at <= ?1")?;
    let ids = stmt
        .query_map([cutoff], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for id in &ids {
        conn.execute("DELETE FROM profiles WHERE id = ?1", [id])?;
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::migrate(&conn, db::APP_MIGRATIONS).unwrap();
        conn
    }

    #[test]
    fn first_run_creates_main_profile_and_activates_it() {
        let conn = conn();
        let active = ensure_default(&conn, 100).unwrap();
        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].is_main);
        assert_eq!(all[0].id, active);
        // Calling again changes nothing.
        assert_eq!(ensure_default(&conn, 200).unwrap(), active);
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn stale_active_id_falls_back_to_main() {
        let conn = conn();
        let main = ensure_default(&conn, 0).unwrap();
        conn.execute(
            "UPDATE app_meta SET value = 'missing' WHERE key = 'active_profile_id'",
            [],
        )
        .unwrap();
        assert_eq!(ensure_default(&conn, 0).unwrap(), main);
    }

    #[test]
    fn create_validates_and_rejects_duplicates() {
        let conn = conn();
        ensure_default(&conn, 0).unwrap();
        assert!(create(&conn, "   ", 1).is_err());
        assert!(create(&conn, &"x".repeat(41), 1).is_err());
        assert!(create(&conn, "main", 1).is_err());
        let p = create(&conn, "  Work  ", 1).unwrap();
        assert_eq!(p.name, "Work");
        assert!(!p.is_main);
        assert!(create(&conn, "WORK", 2).is_err());
    }

    #[test]
    fn rename_allows_same_profile_case_change() {
        let conn = conn();
        ensure_default(&conn, 0).unwrap();
        let p = create(&conn, "Work", 1).unwrap();
        assert_eq!(rename(&conn, &p.id, "WORK").unwrap().name, "WORK");
        assert!(rename(&conn, &p.id, "Main").is_err());
        assert!(rename(&conn, "nope", "X").is_err());
    }

    #[test]
    fn main_profile_cannot_be_deleted_until_replaced() {
        let conn = conn();
        let main = ensure_default(&conn, 0).unwrap();
        let other = create(&conn, "Other", 1).unwrap();
        assert!(delete(&conn, &main, 2).is_err());

        set_main(&conn, &other.id).unwrap();
        let all = list(&conn).unwrap();
        assert_eq!(all.iter().filter(|p| p.is_main).count(), 1);
        assert!(all.iter().find(|p| p.id == other.id).unwrap().is_main);

        // The former main was active; deleting it switches to the new main.
        let active = delete(&conn, &main, 3).unwrap();
        assert_eq!(active, other.id);
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn deleting_inactive_profile_keeps_active() {
        let conn = conn();
        let main = ensure_default(&conn, 0).unwrap();
        let other = create(&conn, "Other", 1).unwrap();
        assert_eq!(delete(&conn, &other.id, 2).unwrap(), main);
    }

    #[test]
    fn deleted_profiles_can_be_restored_with_unique_name() {
        let conn = conn();
        ensure_default(&conn, 0).unwrap();
        let p = create(&conn, "Work", 1).unwrap();
        delete(&conn, &p.id, 10).unwrap();
        assert!(set_active(&conn, &p.id).is_err());

        let bin = list_deleted(&conn, 10 + 5 * SECONDS_PER_DAY).unwrap();
        assert_eq!(bin.len(), 1);
        assert_eq!(bin[0].days_remaining, 85);

        create(&conn, "Work", 11).unwrap();
        let restored = restore(&conn, &p.id).unwrap();
        assert_eq!(restored.name, "Work (2)");
        assert!(list_deleted(&conn, 12).unwrap().is_empty());
        assert!(restore(&conn, &p.id).is_err());
    }

    #[test]
    fn only_deleted_profiles_can_be_purged_early() {
        let conn = conn();
        let main = ensure_default(&conn, 0).unwrap();
        let other = create(&conn, "Other", 1).unwrap();
        assert!(
            purge(&conn, &main).is_err(),
            "an active profile cannot be purged"
        );
        assert!(
            purge(&conn, &other.id).is_err(),
            "nor one that is not deleted"
        );
        delete(&conn, &other.id, 2).unwrap();
        purge(&conn, &other.id).unwrap();
        assert!(list_deleted(&conn, 3).unwrap().is_empty());
        assert!(restore(&conn, &other.id).is_err());
    }

    #[test]
    fn purge_removes_only_profiles_past_retention() {
        let conn = conn();
        ensure_default(&conn, 0).unwrap();
        let old = create(&conn, "Old", 1).unwrap();
        let recent = create(&conn, "Recent", 1).unwrap();
        let now = 200 * SECONDS_PER_DAY;
        delete(&conn, &old.id, now - 90 * SECONDS_PER_DAY).unwrap();
        delete(&conn, &recent.id, now - 89 * SECONDS_PER_DAY).unwrap();

        assert_eq!(purge_expired(&conn, now).unwrap(), vec![old.id.clone()]);
        let bin = list_deleted(&conn, now).unwrap();
        assert_eq!(bin.len(), 1);
        assert_eq!(bin[0].id, recent.id);
        assert_eq!(bin[0].days_remaining, 1);
        // Active profiles are never purged.
        assert_eq!(list(&conn).unwrap().len(), 1);
    }
}
