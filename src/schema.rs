// @generated automatically by Diesel CLI.

diesel::table! {
    account_transfers (id) {
        id -> Integer,
        from_account_id -> Integer,
        to_account_id -> Integer,
        amount -> BigInt,
        description -> Nullable<Text>,
        date -> Timestamp,
        user_id -> Integer,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    accounts (id) {
        id -> Integer,
        name -> Text,
        balance -> BigInt,
        bank_name -> Text,
        user_id -> Integer,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    categories (id) {
        id -> Integer,
        name -> Text,
        description -> Nullable<Text>,
        user_id -> Nullable<Integer>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    transactions (id) {
        id -> Integer,
        amount -> BigInt,
        description -> Nullable<Text>,
        date -> Timestamp,
        #[sql_name = "type"]
        type_ -> Text,
        account_id -> Integer,
        transfer_id -> Nullable<Integer>,
        category_id -> Nullable<Integer>,
        user_id -> Integer,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    users (id) {
        id -> Integer,
        name -> Text,
        email -> Text,
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
