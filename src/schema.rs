// @generated automatically by Diesel CLI.

diesel::table! {
    account_transfers (id) {
        id -> Text,
        from_account_id -> Text,
        to_account_id -> Text,
        amount -> BigInt,
        description -> Nullable<Text>,
        date -> Timestamp,
        user_id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    accounts (id) {
        id -> Text,
        name -> Text,
        icon -> Nullable<Text>,
        balance -> BigInt,
        bank_name -> Text,
        user_id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    categories (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        color -> Nullable<Text>,
        icon -> Nullable<Text>,
        user_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    transactions (id) {
        id -> Text,
        amount -> BigInt,
        description -> Nullable<Text>,
        date -> Timestamp,
        #[sql_name = "type"]
        type_ -> Text,
        account_id -> Text,
        transfer_id -> Nullable<Text>,
        category_id -> Nullable<Text>,
        user_id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    users (id) {
        id -> Text,
        name -> Text,
        currency -> Nullable<Text>,
        email -> Text,
        password -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    account_transfers,
    accounts,
    categories,
    transactions,
    users,
);
