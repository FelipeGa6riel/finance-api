DROP INDEX IF EXISTS `idx_transactions_user_date`;
CREATE INDEX `idx_transactions_user_date` ON `transactions`(`user_id`, `date` DESC, `created_at` DESC) WHERE `deleted_at` IS NULL;
CREATE INDEX IF NOT EXISTS `idx_transfers_user_date` ON `account_transfers`(`user_id`, `date` DESC, `created_at` DESC) WHERE `deleted_at` IS NULL;
