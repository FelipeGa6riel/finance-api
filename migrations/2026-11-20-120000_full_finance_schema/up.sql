PRAGMA foreign_keys = OFF;
BEGIN TRANSACTION;

CREATE TABLE `users_new` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `currency` TEXT DEFAULT 'BRL',
    `email` TEXT NOT NULL UNIQUE,
    `password` TEXT,
    `cycle_start_day` INTEGER NOT NULL DEFAULT 20,
    `default_savings_percent` REAL NOT NULL DEFAULT 35.0,
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);

INSERT INTO `users_new` (`id`, `name`, `currency`, `email`, `password`, `cycle_start_day`, `default_savings_percent`, `deleted_at`, `created_at`, `updated_at`)
SELECT `id`, `name`, COALESCE(`currency`, 'BRL'), `email`, `password`, 20, 35.0, NULL, `created_at`, `updated_at`
FROM `users`;

DROP TABLE `users`;
ALTER TABLE `users_new` RENAME TO `users`;

CREATE TABLE `accounts_new` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `icon` TEXT,
    `balance` BIGINT NOT NULL,
    `bank_name` TEXT NOT NULL,
    `account_type` TEXT NOT NULL DEFAULT 'checking',
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);

INSERT INTO `accounts_new` (`id`, `name`, `icon`, `balance`, `bank_name`, `account_type`, `user_id`, `deleted_at`, `created_at`, `updated_at`)
SELECT `id`, `name`, `icon`, `balance`, `bank_name`,
    CASE
        WHEN LOWER(`name`) LIKE '%invest%' OR LOWER(`bank_name`) LIKE '%invest%' THEN 'investment'
        WHEN LOWER(`name`) IN ('cash', 'dinheiro', 'carteira') THEN 'cash'
        ELSE 'checking'
    END,
    `user_id`, NULL, `created_at`, `updated_at`
FROM `accounts`
WHERE `user_id` IN (SELECT `id` FROM `users`);

DROP TABLE `accounts`;
ALTER TABLE `accounts_new` RENAME TO `accounts`;
CREATE UNIQUE INDEX `idx_accounts_user_name` ON `accounts`(`user_id`, `name`) WHERE `deleted_at` IS NULL;

CREATE TABLE `categories_new` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `description` TEXT,
    `color` TEXT,
    `icon` TEXT,
    `category_type` TEXT NOT NULL DEFAULT 'expense',
    `is_fixed` BOOLEAN NOT NULL DEFAULT 0,
    `is_salary` BOOLEAN NOT NULL DEFAULT 0,
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);

INSERT INTO `categories_new` (`id`, `name`, `description`, `color`, `icon`, `category_type`, `is_fixed`, `is_salary`, `user_id`, `deleted_at`, `created_at`, `updated_at`)
SELECT `id`, `name`, `description`, `color`, `icon`,
    CASE WHEN LOWER(`name`) IN ('salary', 'salário', 'freelance', 'décimo', 'decimo') THEN 'income' ELSE 'expense' END,
    CASE WHEN LOWER(`name`) IN ('despesa fixa', 'fixed') THEN 1 ELSE 0 END,
    CASE WHEN LOWER(`name`) IN ('salary', 'salário') THEN 1 ELSE 0 END,
    `user_id`, NULL, `created_at`, `updated_at`
FROM `categories`
WHERE `user_id` IS NOT NULL AND `user_id` IN (SELECT `id` FROM `users`);

DROP TABLE `categories`;
ALTER TABLE `categories_new` RENAME TO `categories`;
CREATE UNIQUE INDEX `idx_categories_user_name_type` ON `categories`(`user_id`, `name`, `category_type`) WHERE `deleted_at` IS NULL;

CREATE TABLE `recurring_rules` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `description` TEXT NOT NULL,
    `default_amount` BIGINT NOT NULL,
    `type` TEXT NOT NULL DEFAULT 'expense',
    `payment_method` TEXT NOT NULL DEFAULT 'pix',
    `account_id` TEXT NOT NULL REFERENCES `accounts`(`id`),
    `category_id` TEXT REFERENCES `categories`(`id`),
    `day_of_month` INTEGER NOT NULL,
    `is_variable_amount` BOOLEAN NOT NULL DEFAULT 0,
    `is_fixed` BOOLEAN NOT NULL DEFAULT 1,
    `active` BOOLEAN NOT NULL DEFAULT 1,
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);

CREATE TABLE `account_transfers_new` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `from_account_id` TEXT NOT NULL REFERENCES `accounts`(`id`),
    `to_account_id` TEXT REFERENCES `accounts`(`id`),
    `amount` BIGINT NOT NULL,
    `description` TEXT,
    `date` TIMESTAMP NOT NULL,
    `transfer_kind` TEXT NOT NULL DEFAULT 'general',
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);

INSERT INTO `account_transfers_new` (`id`, `from_account_id`, `to_account_id`, `amount`, `description`, `date`, `transfer_kind`, `user_id`, `deleted_at`, `created_at`, `updated_at`)
SELECT `id`, `from_account_id`, `to_account_id`, `amount`, `description`, `date`,
    CASE WHEN LOWER(COALESCE(`description`, '')) LIKE '%invest%' THEN 'investment' ELSE 'general' END,
    `user_id`, NULL, `created_at`, `updated_at`
FROM `account_transfers`
WHERE `user_id` IN (SELECT `id` FROM `users`)
  AND `from_account_id` IN (SELECT `id` FROM `accounts`)
  AND `to_account_id` IN (SELECT `id` FROM `accounts`);

DROP TABLE `account_transfers`;
ALTER TABLE `account_transfers_new` RENAME TO `account_transfers`;

CREATE TABLE `transactions_new` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `amount` BIGINT NOT NULL,
    `description` TEXT,
    `date` TIMESTAMP NOT NULL,
    `type` TEXT NOT NULL,
    `payment_method` TEXT NOT NULL DEFAULT 'pix',
    `card_name` TEXT,
    `account_id` TEXT NOT NULL REFERENCES `accounts`(`id`),
    `transfer_id` TEXT REFERENCES `account_transfers`(`id`),
    `category_id` TEXT REFERENCES `categories`(`id`),
    `is_fixed` BOOLEAN,
    `is_informational` BOOLEAN NOT NULL DEFAULT 0,
    `credit_paid` BOOLEAN NOT NULL DEFAULT 0,
    `installment_group_id` TEXT,
    `installment_number` INTEGER,
    `installment_total` INTEGER,
    `recurring_rule_id` TEXT REFERENCES `recurring_rules`(`id`),
    `needs_confirmation` BOOLEAN NOT NULL DEFAULT 0,
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);

INSERT INTO `transactions_new` (`id`, `amount`, `description`, `date`, `type`, `payment_method`, `card_name`, `account_id`, `transfer_id`, `category_id`, `is_fixed`, `is_informational`, `credit_paid`, `installment_group_id`, `installment_number`, `installment_total`, `recurring_rule_id`, `needs_confirmation`, `user_id`, `deleted_at`, `created_at`, `updated_at`)
SELECT `id`, `amount`, `description`, `date`, `type`, 'pix', NULL, `account_id`,
    CASE WHEN `transfer_id` IN (SELECT `id` FROM `account_transfers`) THEN `transfer_id` ELSE NULL END,
    CASE WHEN `category_id` IN (SELECT `id` FROM `categories`) THEN `category_id` ELSE NULL END,
    NULL, 0, 0, NULL, NULL, NULL, NULL, 0, `user_id`, NULL, `created_at`, `updated_at`
FROM `transactions`
WHERE `user_id` IN (SELECT `id` FROM `users`)
  AND `account_id` IN (SELECT `id` FROM `accounts`);

DROP TABLE `transactions`;
ALTER TABLE `transactions_new` RENAME TO `transactions`;
CREATE INDEX `idx_transactions_user_date` ON `transactions`(`user_id`, `date`) WHERE `deleted_at` IS NULL;

CREATE TABLE `tags` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `color` TEXT,
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `deleted_at` TIMESTAMP,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL
);
CREATE UNIQUE INDEX `idx_tags_user_name` ON `tags`(`user_id`, `name`) WHERE `deleted_at` IS NULL;

CREATE TABLE `transaction_tags` (
    `transaction_id` TEXT NOT NULL REFERENCES `transactions`(`id`),
    `tag_id` TEXT NOT NULL REFERENCES `tags`(`id`),
    PRIMARY KEY (`transaction_id`, `tag_id`)
);

CREATE TABLE `transfer_tags` (
    `transfer_id` TEXT NOT NULL REFERENCES `account_transfers`(`id`),
    `tag_id` TEXT NOT NULL REFERENCES `tags`(`id`),
    PRIMARY KEY (`transfer_id`, `tag_id`)
);

CREATE TABLE `cycle_configs` (
    `id` TEXT NOT NULL PRIMARY KEY,
    `user_id` TEXT NOT NULL REFERENCES `users`(`id`),
    `cycle_key` TEXT NOT NULL,
    `start_date` TEXT NOT NULL,
    `end_date` TEXT NOT NULL,
    `label` TEXT,
    `savings_mode` TEXT NOT NULL DEFAULT 'percent',
    `savings_percent` REAL,
    `savings_fixed_amount` BIGINT,
    `notes` TEXT,
    `created_at` TIMESTAMP NOT NULL,
    `updated_at` TIMESTAMP NOT NULL,
    UNIQUE(`user_id`, `cycle_key`)
);

COMMIT;
PRAGMA foreign_keys = ON;
