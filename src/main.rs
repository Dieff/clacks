use actix::{Actor, System};
use actix_web::{guard, middleware, web, App, HttpServer};
use env_logger;
use graphql_parser::parse_schema;
use log::info;

#[macro_use]
extern crate diesel;
use diesel::mysql::MysqlConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenv;

/// JWT validation
mod auth;
/// Contains the configuration for the application
mod config;
mod gql_context;
mod gqln;
mod models;
mod resolvers;
mod routes;
mod schema;
/// Handle the statefule aspect of ws connections and graphql subscriptions.
mod ws_actors;
/// Contains serializable structs that represent the messages sent across
/// a websocket on a graphql subscription server.
mod ws_messages;
use routes::*;

use gqln::*;

fn main() -> std::io::Result<()> {
    // read the .env and populate std::env
    dotenv::dotenv().ok();

    // set the env var RUST_LOG to "actix_web" to see access logs
    env_logger::init();

    // Load up our graphql schema and set some resolvers
    let schema =
        parse_schema(include_str!("../schema.graphql")).expect("could not parse gql schema");

    // Get app config
    let config = config::AppConfig::new();

    // the DB pool allows connections to the mysql db to be shared across threads
    let manager = ConnectionManager::<MysqlConnection>::new(config.db_url.clone().unwrap());
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");

    let mut gqschema = GqlSchema::new(schema).unwrap();
    gqschema
        .add_resolvers(vec![
            Resolver::new(
                Box::new(resolvers::mutation_create_message),
                "Mutation",
                "createMessage",
            ),
            Resolver::new(
                Box::new(resolvers::subscription_message),
                "Subscription",
                "message",
            ),
            Resolver::new(Box::new(resolvers::query_me), "Query", "me"),
        ])
        .unwrap();

    let ws_tracker = ws_actors::ConnectionTracker::new(gqschema.clone(), pool.clone());
    let gql_context = GqlRouteContext::new(gqschema, pool.clone());
    let api_context = ApiContext {
        db: pool.clone(),
        config: config.clone(),
    };

    // start the runtime to allow actix actors to handle events
    let actix_sys = System::new("main");

    // can only start the tracker once the system is up
    let tracker_addr = ws_tracker.start();

    let port = config.graphql_port;
    let man_port = config.management_port;

    // Starting the server creates more actors
    // graphql clients
    HttpServer::new(move || {
        App::new()
            .data(pool.clone())
            .data(gql_context.clone())
            .data(tracker_addr.clone())
            .data(config.clone())
            .route(
                "/graphql",
                web::post()
                    .to(r_graphql_post)
                    .guard(guard::Header("content-type", "application/json")),
            )
            .route(
                "/graphql",
                web::get()
                    .to(r_wsspawn)
                    .guard(guard::Header("upgrade", "websocket")),
            )
            .route(
                "/graphql",
                web::get()
                    .to(r_graphql_get)
                    .guard(guard::Header("content-type", "application/json")),
            )
            .wrap(middleware::Logger::default())
    })
    .bind(format!("0.0.0.0:{}", port))?
    .start();

    // server management
    HttpServer::new(move || {
        App::new().wrap(middleware::Logger::default()).service(
            web::scope("/api/v1")
                .data(api_context.clone())
                .route("/healthz", web::get().to(r_health))
                .route("/channel", web::get().to(r_get_channels)) // view channels
                .route("/channel", web::post().to(r_create_channel)) // create channel
                .route("/channel/{channelId}", web::get().to(r_get_channel_info))
                .route("/channel/{channelId}", web::delete().to(r_delete_channel))
                .route(
                    "/channel/{channelId}/users",
                    web::get().to(r_get_channel_users),
                )
                .route("/channel/{channelId}/users", web::put().to(r_add_user))
                .route(
                    "/channel/{channelId}/{uid}",
                    web::delete().to(r_remove_user),
                )
                .route("/jwt/{uid}", web::get().to(r_get_jwt)),
        )
    })
    .bind(format!("0.0.0.0:{}", man_port))?
    .start();

    info!("Time to start server.");
    actix_sys.run()?;
    Ok(())
}
