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

## Developing

Here are some steps to get up and running:
1. Install rust, cargo, mysql dev packages
2. Start the docker-compose file to run the backend database: `docker-compose up -d`
3. Initialize the new database with the `diesel` cli
  - `cargo install diesel`
  - `diesel database setup --database-url='mysql://diesel::test123@[::1]/chat'`
4. Create your own `.env` file to configure the application. Start with `.env-example`
5. You can use vscode with the included configuration to get debugging support. You will need the following things:
  - The vscode extension `vadimcn.vscode-lldb`
  - lldb
6. Run some tests: `cargo test`, or run the app: `cargo run`


## Running
Run Clacks quickly and easily with Docker. The Dockerfile included in the repo will build
