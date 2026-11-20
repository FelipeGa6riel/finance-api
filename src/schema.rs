// @generated automatically by Diesel CLI.

diesel::table! {
    account_transfers (id) {
        id -> Text,
        from_account_id -> Text,
        to_account_id -> Nullable<Text>,
        amount -> BigInt,
        description -> Nullable<Text>,
        date -> Timestamp,
        transfer_kind -> Text,
        user_id -> Text,
        deleted_at -> Nullable<Timestamp>,
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
        account_type -> Text,
        user_id -> Text,
        deleted_at -> Nullable<Timestamp>,
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
        category_type -> Text,
        is_fixed -> Bool,
        is_salary -> Bool,
        user_id -> Text,
        deleted_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    cycle_configs (id) {
        id -> Text,
        user_id -> Text,
        cycle_key -> Text,
        start_date -> Text,
        end_date -> Text,
        label -> Nullable<Text>,
        savings_mode -> Text,
        savings_percent -> Nullable<Float>,
        savings_fixed_amount -> Nullable<BigInt>,
        notes -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    recurring_rules (id) {
        id -> Text,
        user_id -> Text,
        description -> Text,
        default_amount -> BigInt,
        #[sql_name = "type"]
        type_ -> Text,
        payment_method -> Text,
        account_id -> Text,
        category_id -> Nullable<Text>,
        day_of_month -> Integer,
        is_variable_amount -> Bool,
        is_fixed -> Bool,
        active -> Bool,
        deleted_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    tags (id) {
        id -> Text,
        name -> Text,
        color -> Nullable<Text>,
        user_id -> Text,
        deleted_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    transaction_tags (transaction_id, tag_id) {
        transaction_id -> Text,
        tag_id -> Text,
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
        payment_method -> Text,
        card_name -> Nullable<Text>,
        account_id -> Text,
        transfer_id -> Nullable<Text>,
        category_id -> Nullable<Text>,
        is_fixed -> Nullable<Bool>,
        is_informational -> Bool,
        credit_paid -> Bool,
        installment_group_id -> Nullable<Text>,
        installment_number -> Nullable<Integer>,
        installment_total -> Nullable<Integer>,
        recurring_rule_id -> Nullable<Text>,
        needs_confirmation -> Bool,
        user_id -> Text,
        deleted_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    transfer_tags (transfer_id, tag_id) {
        transfer_id -> Text,
        tag_id -> Text,
    }
}

diesel::table! {
    users (id) {
        id -> Text,
        name -> Text,
        currency -> Nullable<Text>,
        email -> Text,
        password -> Nullable<Text>,
        cycle_start_day -> Integer,
        default_savings_percent -> Float,
        deleted_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::joinable!(account_transfers -> users (user_id));
diesel::joinable!(accounts -> users (user_id));
diesel::joinable!(categories -> users (user_id));
diesel::joinable!(cycle_configs -> users (user_id));
diesel::joinable!(recurring_rules -> accounts (account_id));
diesel::joinable!(recurring_rules -> categories (category_id));
diesel::joinable!(recurring_rules -> users (user_id));
diesel::joinable!(tags -> users (user_id));
diesel::joinable!(transaction_tags -> tags (tag_id));
diesel::joinable!(transaction_tags -> transactions (transaction_id));
diesel::joinable!(transactions -> account_transfers (transfer_id));
diesel::joinable!(transactions -> accounts (account_id));
diesel::joinable!(transactions -> categories (category_id));
diesel::joinable!(transactions -> recurring_rules (recurring_rule_id));
diesel::joinable!(transactions -> users (user_id));
diesel::joinable!(transfer_tags -> account_transfers (transfer_id));
diesel::joinable!(transfer_tags -> tags (tag_id));

diesel::allow_tables_to_appear_in_same_query!(
    account_transfers,
    accounts,
    categories,
    cycle_configs,
    recurring_rules,
    tags,
    transaction_tags,
    transactions,
    transfer_tags,
    users,
);
