# Regras do Workspace — app-finance (Backend)

## Caminhos Ativos (Fonte de Verdade)
- Backend ativo: `/home/felipe/Documents/projects/app-finance/finance-api`
- Frontend ativo: `/home/felipe/Documents/projects/app-finance/finance-app`
- Vault pessoal (Obsidian): `/home/felipe/Documents/projects/vault/02-areas/finance`
  - **RESTRIÇÃO RÍGIDA:** Nunca escrever, mover, renomear, formatar ou rodar comandos git de escrita no vault. Apenas leitura (ex.: `import_vault`).
- Nunca editar cópias legadas fora de `~/Documents/projects/app-finance/` sem pedido explícito.

## Arquitetura e Segurança (App Privado Multi-User)
- Este app é privado (não é SaaS): **não adicionar** billing, planos, assinaturas, paywall, quotas comerciais, orgs/workspaces ou cadastro público HTTP (`POST /users`).
- **Manter sempre** autenticação JWT (`SECRET_KEY` obrigatória no boot), criação de usuários via CLI `user_admin`, soft delete (`deleted_at`) e isolamento estrito por `user_id` em toda query e referência (`account_id`, `category_id`, `tag_id`, `transfer`, `recurrence`).

## Convenções de Domínio e API
- **Reserva percentual (`%`):** calculada sobre o **Total de Receitas** (`total_income`) do ciclo.
- **Rede local / WSL2 + Emulador Android:** manter o servidor fazendo bind em `0.0.0.0:8007` em desenvolvimento para permitir acesso a partir do Windows/Android Studio Emulator (`10.0.2.2:8007` ou `adb reverse`).
