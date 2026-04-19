//! Persistent application database module.

pub mod migrations;
pub mod schema;

pub use schema::init_app_db;
