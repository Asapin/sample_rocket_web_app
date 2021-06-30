#![allow(proc_macro_derive_resolution_fallback, unused_attributes)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate rocket_sync_db_pools;
#[macro_use]
extern crate diesel_migrations;

use askama::Template;
use diesel::OptionalExtension;
use diesel::{RunQueryDsl, QueryDsl};
use dotenv::dotenv;
use errors::CustomError;
use models::Confession;
use models::NewConfession;
use rocket::Build;
use rocket::Rocket;
use rocket::fairing::AdHoc;
use rocket::figment::Figment;
use rocket::figment::map;
use rocket::figment::value::Map;
use rocket::figment::value::Value;
use rocket::response::content;
use rocket::response::status::Created;
use rocket::response::status::NotFound;
use rocket::fs::NamedFile;
use rocket::serde::json::Json;
use std::env;
use std::env::VarError;
use std::num::ParseIntError;
use std::path::PathBuf;
use diesel::pg::PgConnection;
use thiserror::Error;
use serde::{Deserialize, Serialize};

mod schema;
mod models;
mod errors;

no_arg_sql_function!(RANDOM, (), "Represents the sql RANDOM() function");

#[derive(Template)]
#[template(path = "index.html")]
struct HomepageTemplate {
    confession: Option<String>,
    total_confessions: i64
}

#[derive(Deserialize)]
struct ConfessionJson {
    content: String,
}

#[derive(Serialize)]
struct NewConfessionResponse {
    confession: Confession,
}

#[database("confessions_db")]
pub struct DBPool(PgConnection);

#[derive(Error, Debug)]
enum DbLoadErrors {
    #[error("DB URL is not found in the env parameters: {0}")]
    DbUrlNotFound(#[source] VarError),
    #[error("DB Connection Pool Size is not found in the env parameters: {0}")]
    DbConnectionPoolSizeNotFound(#[source] VarError),
    #[error("DB Connection Pool Size is found but couldn't be parsed: {0}")]
    DbConnectionPoolSizeParse(#[source] ParseIntError)
}

#[get("/")]
async fn root(conn: DBPool) -> Result<content::Html<String>, CustomError> {
    let confession_from_db: Option<Confession> = conn
        .run(|c| get_random_confession(c))
        .await?;
    let confession_count = conn.run(|c| schema::confessions::table.count().get_result(c)).await?;

    let template = HomepageTemplate {
        confession: confession_from_db.map(|c| c.confession),
        total_confessions: confession_count
    };

    let response = content::Html(template.to_string());
    Ok(response)
}

#[get("/<path..>")]
async fn static_files(path: PathBuf) -> Result<NamedFile, NotFound<String>> {
    let path = PathBuf::from("site").join(path);
    NamedFile::open(path)
        .await
        .map_err(|e| NotFound(e.to_string()))
}

#[post("/confession", format = "json", data = "<json_confession>")]
async fn post_confession(
    conn: DBPool,
    json_confession: Json<ConfessionJson>
) -> Result<Created<Json<NewConfessionResponse>>, CustomError> {
    let new_confession = conn.run(move |c| {
        diesel::insert_into(schema::confessions::table)
            .values(NewConfession { confession: &json_confession.content })
            .get_result(c)
    }).await?;

    let response = NewConfessionResponse {
        confession: new_confession
    };

    Ok(Created::new("/confession").body(Json(response)))
}

fn get_random_confession(conn: &PgConnection) -> Result<Option<Confession>, diesel::result::Error> {
    schema::confessions::table
        .order(RANDOM)
        .limit(1)
        .first::<Confession>(conn)
        .optional()
}

#[get("/confession", format = "json")]
async fn get_confession(conn: DBPool) -> Result<Json<Option<Confession>>, CustomError> {
    let confession: Option<Confession> = conn.run(|c| get_random_confession(c)).await?;

    Ok(Json(confession))
}

fn load_db_params() -> Result<Figment, DbLoadErrors> {
    let db_url = env::var("DATABASE_URL")
        .map_err(|e| DbLoadErrors::DbUrlNotFound(e))?;
    let db_conn_pool_size = env::var("CONNECTION_POOL_SIZE")
        .map_err(|e| DbLoadErrors::DbConnectionPoolSizeNotFound(e))?;
    let db_conn_pool_size = i32::from_str_radix(&db_conn_pool_size, 10)
        .map_err(|e| DbLoadErrors::DbConnectionPoolSizeParse(e))?;

    let db: Map<_, Value> = map! {
        "url" => db_url.into(),
        "pool_size" => db_conn_pool_size.into()
    };

    let figment = rocket::Config::figment()
    .merge(
        ("databases", map!["confessions_db" => db])
    );

    Ok(figment)
}

async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    embed_migrations!();

    let conn: DBPool = DBPool::get_one(&rocket)
        .await
        .expect("database connections");
    conn.run(|c| embedded_migrations::run(c))
        .await
        .expect("can run migrations");

    rocket
}

#[launch]
fn rocket() -> _ {
    dotenv().ok();

    let figment = match load_db_params() {
        Ok(figment) => figment,
        Err(e) => {
            panic!("Error: {}", e);
        },
    };

    rocket::custom(figment)
        .mount("/", routes![root, static_files])
        .mount("/api", routes![post_confession, get_confession])
        .attach(DBPool::fairing())
        .attach(AdHoc::on_ignite("Run migrations", run_migrations))
}