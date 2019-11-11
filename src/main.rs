use actix::{Actor, Addr, System};
use actix_web::{
    guard, http::StatusCode, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer,
    Responder,
};
use actix_web_actors::ws;
use env_logger;
use graphql_parser::parse_schema;
use log::info;
use std::env;

#[macro_use]
extern crate diesel;
use diesel::mysql::MysqlConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenv;

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

// When a new websocket request comes in, start a new actor
fn r_wstest(
    req: HttpRequest,
    stream: web::Payload,
    recip: web::Data<Addr<ws_actors::ConnectionTracker>>,
) -> Result<HttpResponse, Error> {
    info!("New websocket request. Some subscriptions will be next.");
    let id = req
        .headers()
        .get("Authorization")
        .map(|i| i.as_bytes().to_vec());
    let handler = WsHandler::new(recip.get_ref().to_owned(), id);
    ws::start_with_protocols(handler, &["graphql-ws"], &req, stream)
}

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

// The main POST endpoint for graphql queries
// such as reading data, sending messages
fn r_graphql(
    payload: web::Json<GqlRequest>,
    gql_ctx: web::Data<GqlRouteContext>,
    tracker: web::Data<Addr<ws_actors::ConnectionTracker>>,
) -> impl Responder {
    let context = GqlContext::new(
        gql_ctx.db.clone(),
        "Asdf".to_owned(),
        tracker.get_ref().clone(),
    );
    let gql_resp = gql_ctx.schema.resolve(context, payload.0, None);
    HttpResponse::Ok().json(GqlResponse::from(gql_resp))
}

// graphql is also supposed to be able to handle GET requests
fn r_graphql_get(
    payload: web::Query<GqlRequest>,
    gql_ctx: web::Data<GqlRouteContext>,
    tracker: web::Data<Addr<ws_actors::ConnectionTracker>>,
) -> impl Responder {
    let context = GqlContext::new(
        gql_ctx.db.clone(),
        "Asdf".to_owned(),
        tracker.get_ref().clone(),
    );
    let gql_resp = gql_ctx.schema.resolve(context, payload.0, None);
    HttpResponse::Ok().json(GqlResponse::from(gql_resp))
}

fn main() -> std::io::Result<()> {
    // read the .env and populate std::env
    dotenv::dotenv().ok();

    // set the env var RUST_LOG to "actix_web" to see access logs
    env_logger::init();

    // Load up our graphql schema and set some resolvers
    let schema =
        parse_schema(include_str!("../schema.graphql")).expect("could not parse gql schema");

    // recover the db connection string
    let db_url = env::var("DATABASE_URL").expect("could not find env var $DATABASE_URL");

    // the DB pool allows connections to the mysql db to be shared across threads
    let manager = ConnectionManager::<MysqlConnection>::new(db_url);
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
        ])
        .unwrap();

    let ws_tracker = ws_actors::ConnectionTracker::new(gqschema.clone(), pool.clone());
    let gql_context = GqlRouteContext::new(gqschema, pool.clone());

    // start the runtime to allow actix actors to handle events
    let actix_sys = System::new("main");

    let tracker_addr = ws_tracker.start();

    // Starting the server creates some actors
    HttpServer::new(move || {
        App::new()
            .data(pool.clone())
            .data(gql_context.clone())
            .data(tracker_addr.clone())
            .route("/healthz", web::get().to(r_health))
            .route(
                "/graphql",
                web::post()
                    .to(r_graphql)
                    .guard(guard::Header("content-type", "application/json")),
            )
            .route(
                "/graphql",
                web::get()
                    .to(r_graphql_get)
                    .guard(guard::Header("content-type", "application/json")),
            )
            .route(
                "/graphql",
                web::get()
                    .to(r_wstest)
                    .guard(guard::Header("upgrade", "websocket")),
            )
            .wrap(middleware::Logger::default())
    })
    .bind("0.0.0.0:8000")?
    .start();

    actix_sys.run()?;
    Ok(())
}
