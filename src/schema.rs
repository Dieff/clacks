// This file is auto-generated by Diesel cli

table! {
    channels (id) {
        id -> Integer,
        display_name -> Nullable<Varchar>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    channel_members (id) {
        id -> Integer,
        channel_id -> Integer,
        user -> Varchar,
        user_role -> Nullable<Varchar>,
    }
}

table! {
    messages (id) {
        id -> Integer,
        sender -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        edited -> Nullable<Bool>,
        channel_id -> Integer,
        content -> Nullable<Longtext>,
    }
}

table! {
    message_views (id) {
        id -> Integer,
        message_id -> Integer,
        user -> Varchar,
        created_at -> Timestamp,
    }
}

joinable!(channel_members -> channels (channel_id));
joinable!(message_views -> messages (message_id));
joinable!(messages -> channels (channel_id));

allow_tables_to_appear_in_same_query!(channels, channel_members, messages, message_views,);
