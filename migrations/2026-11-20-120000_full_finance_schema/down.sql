PRAGMA foreign_keys = OFF;
BEGIN TRANSACTION;

DROP TABLE IF EXISTS `cycle_configs`;
DROP TABLE IF EXISTS `transfer_tags`;
DROP TABLE IF EXISTS `transaction_tags`;
DROP TABLE IF EXISTS `tags`;
DROP INDEX IF EXISTS `idx_transactions_user_date`;
DROP INDEX IF EXISTS `idx_categories_user_name_type`;
DROP INDEX IF EXISTS `idx_accounts_user_name`;

CREATE TABLE `transactions_old` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `amount` BIGINT NOT NULL,
    `description` TEXT,
    `date` TIMESTAMP NOT NULL,
    `type` TEXT NOT NULL,
    `account_id` TEXT NOT NULL,
    `transfer_id` TEXT,
    `category_id` TEXT,
    `user_id` TEXT NOT NULL,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);
INSERT INTO `transactions_old` (`id`, `amount`, `description`, `date`, `type`, `account_id`, `transfer_id`, `category_id`, `user_id`, `created_at`, `updated_at`)
SELECT `id`, `amount`, `description`, `date`, `type`, `account_id`, `transfer_id`, `category_id`, `user_id`, `created_at`, `updated_at` FROM `transactions`;
DROP TABLE `transactions`;
ALTER TABLE `transactions_old` RENAME TO `transactions`;

CREATE TABLE `account_transfers_old` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `from_account_id` TEXT NOT NULL,
    `to_account_id` TEXT NOT NULL,
    `amount` BIGINT NOT NULL,
    `description` TEXT,
    `date` TIMESTAMP NOT NULL,
    `user_id` TEXT NOT NULL,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);
INSERT INTO `account_transfers_old` (`id`, `from_account_id`, `to_account_id`, `amount`, `description`, `date`, `user_id`, `created_at`, `updated_at`)
SELECT `id`, `from_account_id`, COALESCE(`to_account_id`, `from_account_id`), `amount`, `description`, `date`, `user_id`, `created_at`, `updated_at` FROM `account_transfers`;
DROP TABLE `account_transfers`;
ALTER TABLE `account_transfers_old` RENAME TO `account_transfers`;

DROP TABLE IF EXISTS `recurring_rules`;

CREATE TABLE `categories_old` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `description` TEXT,
    `color` TEXT,
    `icon` TEXT,
    `user_id` TEXT,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);
INSERT INTO `categories_old` (`id`, `name`, `description`, `color`, `icon`, `user_id`, `created_at`, `updated_at`)
SELECT `id`, `name`, `description`, `color`, `icon`, `user_id`, `created_at`, `updated_at` FROM `categories`;
DROP TABLE `categories`;
ALTER TABLE `categories_old` RENAME TO `categories`;

CREATE TABLE `accounts_old` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `icon` TEXT,
    `balance` BIGINT NOT NULL,
    `bank_name` TEXT NOT NULL,
    `user_id` TEXT NOT NULL,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);
INSERT INTO `accounts_old` (`id`, `name`, `icon`, `balance`, `bank_name`, `user_id`, `created_at`, `updated_at`)
SELECT `id`, `name`, `icon`, `balance`, `bank_name`, `user_id`, `created_at`, `updated_at` FROM `accounts`;
DROP TABLE `accounts`;
ALTER TABLE `accounts_old` RENAME TO `accounts`;

CREATE TABLE `users_old` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `currency` TEXT,
    `email` TEXT NOT NULL,
    `password` TEXT,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);
INSERT INTO `users_old` (`id`, `name`, `currency`, `email`, `password`, `created_at`, `updated_at`)
SELECT `id`, `name`, `currency`, `email`, `password`, `created_at`, `updated_at` FROM `users`;
DROP TABLE `users`;
ALTER TABLE `users_old` RENAME TO `users`;

COMMIT;
PRAGMA foreign_keys = ON;
