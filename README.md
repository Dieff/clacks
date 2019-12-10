# Clacks

A quick and easy backend for real-time chat applications.

Clacks provides
- A graphql endpoint with subscription support. Clients can connect and be notified of new messages.
- An api for automatic configuration from another server or backend.
- Storage for messages, and tracking of message reads
- Channels for conversations between any number of users.

Clacks is supposed to serve as an intermediate project between Fanout and Mattermost. It provides the backend
part of a real-time chat application, which leaving the front-end open for any sort of client. It does not
handle user management, channels

## Running
Run Clacks quickly and easily with Docker. The Dockerfile included in the repo will build an image.

Clacks requires a database backend. At the moment, the only supported database is Mysql (or Mariadb).

### Client port
Graphql clients are expected to connect to the server's client port. This port accepts reqeusts on the `/graphql` endpoint.
Requests should be authorized with the following header:
```
Authorization: bearer <JWT>
```
The JWT is in the format:

```
Header
{
  "alg": "HS256",
  "typ": "JWT"
}

Body

{
  "iss": "APP USER",
  "sub": "USER ID",
  "exp": Unix timestamp,
  "nbf": Unix Timestamp,
  "name": "User's name"
  ...: Optional fields with user info
}

```

To see the graphql API exposed by Clacks, look at `/schema.graphql`.

### Management Port
The non-realtime portion of an application will likely want to setup user channels, or view data about them. For this reason,
a seperate management port with a JSON API allows other servers to send requests to Clacks and update the live configuration.

See `/openapi.yml` for documentation of this API.

## Developing

Here are some steps to get up and running:

1. Install rust, cargo, mysql dev (build dependency) packages
2. Start the docker-compose file to run the backend database: `docker-compose up -d`
3. Initialize the new database with the `diesel` cli
  * `cargo install diesel`
  * `diesel database setup --database-url='mysql://diesel::test123@[::1]/chat'`
4. Create your own `.env` file to configure the application. Start with `.env-example`
5. You can use vscode with the included configuration to get debugging support. You will need the following things:
  * The vscode extension `vadimcn.vscode-lldb`
  * lldb
6. Run some tests: `cargo test`, or run the app: `cargo run`

Clacks is mirrored on both (Github)[https://github.com/dieff/clacks] and (Sourcehut)[https://git.sr.ht/~dieff/Clacks].