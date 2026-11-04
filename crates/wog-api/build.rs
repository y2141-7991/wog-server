// `sqlx::migrate!` embeds the migration files at compile time, but cargo does
// not notice a new or edited .sql file on its own. Rebuild when they change so
// `cargo run` always applies the latest migrations.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
