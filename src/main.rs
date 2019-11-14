use actix::{Actor, Addr, System};
use actix_web::{
    guard, http::StatusCode, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer,
    Responder,
};
use actix_web_actors::ws;
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
mod schema;
/// Handle the statefule aspect of ws connections and graphql subscriptions.
mod ws_actors;
/// Contains serializable structs that represent the messages sent across
/// a websocket on a graphql subscription server.
mod ws_messages;

use gql_context::GqlContext;
use gqln::*;
use models::*;
use ws_actors::WsHandler;

// For standard health checks
fn r_health(db: web::Data<DbPool>) -> impl Responder {
    let conn: &MysqlConnection = &db.get().unwrap();
    let (message, status) = match create_channel(conn, "test123") {
        Ok(channel) => (
            format!("Success. New channel created with id {}", channel.id),
            StatusCode::OK,
        ),
        _ => ("Failure".to_owned(), StatusCode::INTERNAL_SERVER_ERROR),
    };
    HttpResponse::build(status).body(message)
}

// When a new websocket request comes in, start a new actor
fn r_wstest(
    req: HttpRequest,
    stream: web::Payload,
    recip: web::Data<Addr<ws_actors::ConnectionTracker>>,
    config: web::Data<config::AppConfig>,
) -> Result<HttpResponse, Error> {
    info!("New websocket request. Some subscriptions will be next.");
    let id = match req.headers().get("Authorization").map(|i| i.to_str()) {
        Some(Ok(s)) => match auth::decode_jwt(s, &config.jwt_secret.as_ref().unwrap()) {
            Ok(claims) => Some(claims.id),
            _ => None,
        },
        _ => None,
    };

    let handler = WsHandler::new(
        recip.get_ref().to_owned(),
        id,
        config.jwt_secret.clone().unwrap(),
    );
    ws::start_with_protocols(handler, &["graphql-ws"], &req, stream)
}

#[derive(Clone)]
struct GqlRouteContext {
    db: DbPool,
    schema: GqlSchema<GqlContext>,
}

impl GqlRouteContext {
    fn new(schema: GqlSchema<GqlContext>, db: DbPool) -> Self {
        GqlRouteContext { db, schema }
    }
}

fn handle_graphql_req(
    req: &HttpRequest,
    payload: GqlRequest,
    ctx: &web::Data<GqlRouteContext>,
    tracker: &Addr<ws_actors::ConnectionTracker>,
    config: &config::AppConfig,
) -> HttpResponse {
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(jwt) = auth_header.to_str() {
            if let Ok(user_info) = auth::decode_jwt(jwt, &config.jwt_secret.as_ref().unwrap()) {
                let mut context = GqlContext::new(ctx.db.clone(), user_info.id, tracker.to_owned());
                let gql_resp = ctx.schema.resolve(&mut context, payload, None);
                return HttpResponse::Ok().json(GqlResponse::from(gql_resp));
            }
        }
    }
    HttpResponse::Unauthorized().finish()
}

// The main POST endpoint for graphql queries
// such as reading data, sending messages
fn r_graphql_post(
    req: HttpRequest,
    payload: web::Json<GqlRequest>,
    gql_ctx: web::Data<GqlRouteContext>,
    tracker: web::Data<Addr<ws_actors::ConnectionTracker>>,
    config: web::Data<config::AppConfig>,
) -> impl Responder {
    handle_graphql_req(
        &req,
        payload.0,
        &gql_ctx,
        tracker.get_ref(),
        config.get_ref(),
    )
}

// graphql is also supposed to be able to handle GET requests
fn r_graphql_get(
    req: HttpRequest,
    payload: web::Query<GqlRequest>,
    gql_ctx: web::Data<GqlRouteContext>,
    tracker: web::Data<Addr<ws_actors::ConnectionTracker>>,
    config: web::Data<config::AppConfig>,
) -> impl Responder {
    handle_graphql_req(
        &req,
        payload.0,
        &gql_ctx,
        tracker.get_ref(),
        config.get_ref(),
    )
}

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

    // start the runtime to allow actix actors to handle events
    let actix_sys = System::new("main");

    // can only start the tracker once the system is up
    let tracker_addr = ws_tracker.start();

    let port = config.graphql_port;
    // Starting the server creates more actors
    HttpServer::new(move || {
        App::new()
            .data(pool.clone())
            .data(gql_context.clone())
            .data(tracker_addr.clone())
            .data(config.clone())
            .route("/healthz", web::get().to(r_health))
            .route(
                "/graphql",
                web::post()
                    .to(r_graphql_post)
                    .guard(guard::Header("content-type", "application/json")),
            )
            .route(
                "/graphql",
                web::get()
                    .to(r_wstest)
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

    actix_sys.run()?;
    Ok(())
}
