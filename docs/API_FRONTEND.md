# Finance API — Contrato de Endpoints

## 1. Configuração e Autenticação
- Variáveis de ambiente obrigatórias: `DATABASE_URL`, `SECRET_KEY` (a API aborta no boot se ausente ou vazia).
- Opcionais: `BIND_ADDR` (padrão `127.0.0.1:8007`), `CORS_ORIGINS` (origens separadas por vírgula).
- **Rota pública única:** `POST /login` (com rate limit por IP; verifica senha com `bcrypt` e rejeita usuários com `deleted_at != NULL`).
- **Criação de usuários:** `POST /users` foi removida por segurança. Provisionamento é feito via CLI local:
  - `cargo run --bin user_admin -- create --name "..." --email "..." --password "..." [--cycle-start-day 20] [--savings-percent 35]`
  - `cargo run --bin user_admin -- list | deactivate | reactivate | reset-password`
- **Importação do vault (somente leitura):**
  - `cargo run --bin import_vault -- --email "..." [--vault-dir ~/Documents/projects/vault/02-areas/finance/records] [--dry-run]`
- **Rotas protegidas (`/api/*`):** exigem header `Authorization: Bearer <token>`. Todo acesso valida ownership por `user_id == claims.sub` e `deleted_at IS NULL`.

## 2. Convenções
- Valores monetários em centavos (`i64`).
- Datas aceitam `"YYYY-MM-DD HH:MM:SS"`, `"YYYY-MM-DD"` e ISO-8601; respostas retornam `"YYYY-MM-DD HH:MM:SS"`.
- Exclusões usam soft delete (`deleted_at`) com estorno atômico de saldo em transações e transferências.

## 3. Endpoints `/api`
- **Perfil:** `GET /api/me`, `PUT /api/me`, `POST /api/me/password`, `GET /api/users`, `GET /api/users/{id}`, `POST /api/users/{id}`.
- **Contas:** `GET /api/accounts`, `GET /api/accounts/{id}`, `POST /api/accounts` (`account_type`: `checking|savings|investment|cash|credit_card`), `PUT /api/accounts/{id}`, `DELETE /api/accounts/{id}`.
- **Categorias:** `GET /api/categories`, `GET /api/categories/{id}`, `POST /api/categories` (`category_type`: `expense|income`, `is_fixed`, `is_salary`), `PUT /api/categories/{id}`, `DELETE /api/categories/{id}`.
- **Tags:** `GET /api/tags`, `POST /api/tags`, `DELETE /api/tags/{id}`.
- **Transações:** `GET /api/transactions` (filtros: `cycle_key`, `from`, `to`, `account_id`, `category_id`, `payment_method`, `type`, `tag_id`, `search`, `needs_confirmation`, `only_open_credit`), `GET /api/transactions/{id}`, `POST /api/transactions` (retorna `id`, suporta `payment_method`, `card_name`, `is_informational`, `installment_total`, `tag_ids`), `PUT /api/transactions/{id}`, `DELETE /api/transactions/{id}`.
- **Cartão de Crédito (Opção 3):** `POST /api/credit/pay-invoice` (quita compras no crédito selecionadas e debita a conta bancária sem duplicar despesas).
- **Transferências & Investimentos:** `GET /api/transfers`, `POST /api/transfers` (`transfer_kind`: `general|investment|savings|invoice_payment`, `tag_ids`), `DELETE /api/transfers/{id}`.
- **Ciclos & Visão Geral:** `GET /api/cycles`, `GET /api/cycles/summary?cycle=YYYY-MM`, `PUT /api/cycles/{cycle_key}/config`, `GET /api/overview`.
- **Recorrências:** `GET /api/recurrences`, `POST /api/recurrences`, `PUT /api/recurrences/{id}`, `DELETE /api/recurrences/{id}`, `POST /api/recurrences/generate`.
