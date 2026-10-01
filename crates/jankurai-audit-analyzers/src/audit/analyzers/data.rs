use jankurai_audit_kernel::audit::helpers::*;
use jankurai_audit_kernel::audit::scan;
use jankurai_audit_kernel::model::{DimensionResult, FileInfo};

pub fn analyze(ctx: &AuditContext) -> DimensionResult {
    let mut score = 50;
    let mut evidence = vec![];
    let mut notes = vec![];
    let files = product_files(ctx);
    let wrong_layer_db = scan::wrong_layer_db_hits(ctx);
    let has_db_surface = has_prefix(ctx, "db")
        || has_prefix(ctx, "migrations")
        || files.iter().any(|file| file.suffix == ".sql")
        || !wrong_layer_db.is_empty();
    if !has_db_surface
        && ctx
            .all_files
            .iter()
            .any(|f| f.rel_path == "agent/audit-policy.toml")
    {
        return make_dim(
            "Data truth and workflow safety",
            90,
            vec![
                "no adopter product DB surface; standards/tooling repo classification is explicit"
                    .into(),
            ],
            vec![],
        );
    }
    // A repository with product code but no database at all has no data truth to protect:
    // scoring it at the base (50) failed the floor for missing safety it does not need.
    if !has_db_surface && !has_database_marker(ctx, &files) {
        return make_dim(
            "Data truth and workflow safety",
            90,
            vec!["no database surface (no db/ or migrations/, no SQL, no database driver); data-truth checks do not apply".into()],
            vec![],
        );
    }
    if has_prefix(ctx, "db") || files.iter().any(|f| f.suffix == ".sql") {
        score += 15;
        evidence.push("database surface present".into());
    }
    let db_boundary = boundary_manifest(ctx).and_then(|manifest| manifest.db);
    if db_boundary
        .as_ref()
        .map(|db| !db.root_paths.is_empty())
        .unwrap_or(false)
    {
        score += 5;
        evidence.push("structured db boundary manifest present".into());
    }
    if db_boundary
        .as_ref()
        .map(|db| {
            !db.root_paths.is_empty()
                && !db.migration_paths.is_empty()
                && !db.constraint_paths.is_empty()
        })
        .unwrap_or(false)
    {
        score += 5;
        evidence.push("db boundary routes roots, migrations, and constraints".into());
    }
    if has_prefix(ctx, "db/migrations") || has_prefix(ctx, "migrations") {
        score += 10;
        evidence.push("migration directory present".into());
    }
    let db_docs = db_policy_text(ctx);
    if files.iter().any(|f| {
        f.text.to_ascii_lowercase().contains("foreign key")
            || f.text.to_ascii_lowercase().contains("check constraint")
            || f.text.to_ascii_lowercase().contains("row level security")
    }) || db_docs.contains("foreign key")
        || db_docs.contains("check constraint")
        || db_docs.contains("row level security")
    {
        score += 10;
        evidence.push("constraint or RLS language found".into());
    }
    if db_docs.contains("foreign key")
        || db_docs.contains("check constraint")
        || db_docs.contains("row level security")
        || db_docs.contains("rollback")
        || db_docs.contains("backfill")
        || db_docs.contains("lock")
    {
        score += 10;
        evidence.push("db policy docs describe constraints and rollback safety".into());
    }
    if files.iter().any(|f| {
        f.rel_path.starts_with("db/")
            || f.rel_path.starts_with("crates/adapters")
            || f.rel_path.starts_with("adapters")
            || f.rel_path.starts_with("infra")
            || f.rel_path.starts_with("data")
    }) {
        score += 10;
        evidence.push("data access appears compartmentalized".into());
    }
    if !wrong_layer_db.is_empty() {
        score -= 20;
        evidence.push(format!(
            "strict DB boundary violation: {}",
            wrong_layer_db[0].path
        ));
        notes.push("direct DB access leaks out of the data boundary".into());
    }
    make_dim("Data truth and workflow safety", score, evidence, notes)
}

/// Database drivers and clients whose presence in product code means the repository owns data.
const DB_DRIVER_MARKERS: &[&str] = &[
    "sqlx",
    "diesel",
    "rusqlite",
    "tokio_postgres",
    "tokio-postgres",
    "postgres::",
    "sea_orm",
    "sea-orm",
    "redb",
    "sled::",
    "psycopg",
    "sqlalchemy",
    "sqlite3",
    "asyncpg",
    "prisma",
    "knex",
    "typeorm",
    "sequelize",
    "drizzle-orm",
    "better-sqlite3",
    "from 'pg'",
    "from \"pg\"",
    "require('pg')",
    "mongodb",
    "mongoose",
];

/// Database evidence beyond the top-level `db/`/`migrations/`/SQL surface: SQL or a migrations
/// directory anywhere in the tree, a declared DB boundary, or a database driver in a dependency
/// manifest or product code.
fn has_database_marker(ctx: &AuditContext, files: &[FileInfo]) -> bool {
    ctx.all_files
        .iter()
        .any(|f| f.suffix == ".sql" || f.rel_path.contains("/migrations/"))
        || boundary_manifest(ctx)
            .and_then(|manifest| manifest.db)
            .map(|db| !db.root_paths.is_empty())
            .unwrap_or(false)
        || ctx.all_files.iter().any(|f| {
            matches!(
                f.name.as_str(),
                "Cargo.toml" | "package.json" | "pyproject.toml" | "requirements.txt"
            ) && DB_DRIVER_MARKERS.iter().any(|m| f.text.contains(m))
        })
        || files
            .iter()
            .any(|f| DB_DRIVER_MARKERS.iter().any(|m| f.text.contains(m)))
}
