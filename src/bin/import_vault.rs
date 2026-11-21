use diesel::prelude::*;
use dotenvy::dotenv;
use finance_api::cycles::{clean_cycle_label, cycle_bounds_for_key, default_cycle_label};
use finance_api::db::{build_pool, now_utc};
use finance_api::defaults::seed_user_defaults;
use finance_api::models::{
    Account, AccountTransfer, Category, CycleConfig, Tag, Transaction, TransactionTag,
    TransferTag, Type, User, format_ymd, transaction_balance_delta,
};
use finance_api::schema::{
    account_transfers, accounts, categories, cycle_configs, tags, transaction_tags,
    transactions, transfer_tags, users,
};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use time::{Date, Month, PrimitiveDateTime, Time};
use uuid::Uuid;

fn get_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Splits a markdown table row respecting escaped `\|` inside cells (e.g. wikilinks).
fn split_md_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('|')
        .unwrap_or(trimmed)
        .strip_suffix('|')
        .unwrap_or(trimmed);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            cur.push('|');
        } else if c == '|' {
            cells.push(clean_cell(&cur));
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(clean_cell(&cur));
    cells
}

fn clean_cell(raw: &str) -> String {
    raw.replace("<br>", " ")
        .replace("<br/>", " ")
        .replace("<br />", " ")
        .replace("**", "")
        .trim()
        .to_string()
}

fn parse_br_date(raw: &str) -> Option<Date> {
    let parts: Vec<&str> = raw.trim().split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let d: u8 = parts[0].parse().ok()?;
    let m: u8 = parts[1].parse().ok()?;
    let y: i32 = parts[2].parse().ok()?;
    let month = Month::try_from(m).ok()?;
    Date::from_calendar_date(y, month, d).ok()
}

fn parse_cents(raw: &str) -> Option<i64> {
    let cleaned = raw
        .replace("R$", "")
        .replace('*', "")
        .replace('%', "")
        .replace(' ', "");
    if cleaned.is_empty() {
        return None;
    }
    // Handle comma or dot decimal
    let normalized = if cleaned.contains(',') && cleaned.contains('.') {
        cleaned.replace('.', "").replace(',', ".")
    } else if cleaned.contains(',') {
        cleaned.replace(',', ".")
    } else {
        cleaned
    };
    let val: f64 = normalized.parse().ok()?;
    Some((val * 100.0).round() as i64)
}

fn normalize_card_name(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    match t.to_lowercase().as_str() {
        "amazom" | "amazon bradesco" | "bradesco amazon" | "bradesco - amazon prime" => {
            Some("Bradesco Amazon".to_string())
        }
        "mercado pago" => Some("Mercado Pago".to_string()),
        "picpay" => Some("PicPay".to_string()),
        "nubank" => Some("Nubank".to_string()),
        "banco do brasil" => Some("Banco do Brasil".to_string()),
        _ => Some(t.to_string()),
    }
}

fn normalize_category_name(raw: &str) -> String {
    let t = raw.trim();
    match t.to_lowercase().as_str() {
        "lazer" => "Lazer".to_string(),
        "cartão" | "cartao" => "Cartão".to_string(),
        "devoluções" | "devolucoes" | "devolução" | "devolucao" => "Devolução".to_string(),
        "vendas" | "venda" => "Venda".to_string(),
        "decimo" | "décimo" => "Décimo".to_string(),
        "salário" | "salario" => "Salário".to_string(),
        "empréstimo" | "emprestimo" => "Empréstimo".to_string(),
        "combustível" | "combustivel" => "Combustível".to_string(),
        "saúde" | "saude" => "Saúde".to_string(),
        "farmácia" | "farmacia" => "Farmácia".to_string(),
        "mãe" | "mae" => "Mãe".to_string(),
        "investimentos" | "investimento" => "Investimentos".to_string(),
        "outros" | "" => "Outros".to_string(),
        _ => t.to_string(),
    }
}

fn map_payment_method(raw: &str, is_income: bool) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "débito" | "debito" => "debit",
        "pix" => "pix",
        "dinheiro" => "cash",
        "boleto" => "boleto",
        "crédito" | "credito" => "credit",
        "conta salário" | "conta salario" => "salary_account",
        _ if is_income => "pix",
        _ => "pix",
    }
}

fn parse_tags_cell(raw: &str) -> Vec<String> {
    raw.split(|c: char| c.is_whitespace() || c == ',')
        .map(|s| s.trim().trim_start_matches('#').to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

struct ParsedRow {
    date: Date,
    description: String,
    payment_raw: String,
    card_or_dest_raw: String,
    amount_cents: i64,
    category_raw: String,
    tags: Vec<String>,
}

struct ParsedFile {
    cycle_key: String,
    title: String,
    declared_income_total: Option<i64>,
    declared_expense_total: Option<i64>,
    declared_saved: Option<i64>,
    _salary_base: Option<i64>,
    expenses: Vec<ParsedRow>,
    incomes: Vec<ParsedRow>,
}

fn parse_markdown_file(cycle_key: &str, content: &str) -> ParsedFile {
    let mut title = cycle_key.to_string();
    let mut section = "";
    let mut headers: Option<Vec<String>> = None;

    let mut declared_income_total = None;
    let mut declared_expense_total = None;
    let mut declared_saved = None;
    let mut salary_base = None;
    let mut expenses = Vec::new();
    let mut incomes = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            title = clean_cycle_label(trimmed.trim_start_matches('#').trim());
            continue;
        }
        if trimmed.starts_with("## ") {
            headers = None;
            if trimmed.contains("Resumo") {
                section = "resumo";
            } else if trimmed.contains("Lançamentos") {
                section = "expenses";
            } else if trimmed.contains("Entradas") {
                section = "incomes";
            } else {
                section = "";
            }
            continue;
        }
        if !trimmed.starts_with('|') {
            continue;
        }
        let is_sep = trimmed
            .chars()
            .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace());
        if is_sep {
            continue;
        }
        let cells = split_md_cells(trimmed);
        if cells.is_empty() {
            continue;
        }
        if headers.is_none() {
            headers = Some(cells);
            continue;
        }
        let hdr = headers.as_ref().unwrap();

        if section == "resumo" {
            let metric = cells[0].to_lowercase();
            let val_idx = hdr
                .iter()
                .position(|h| h.eq_ignore_ascii_case("Valor"))
                .unwrap_or(cells.len() - 1);
            let val = cells.get(val_idx).and_then(|s| parse_cents(s));
            if metric.starts_with("salário base") {
                salary_base = val;
            } else if metric.starts_with("total de receitas") {
                declared_income_total = val;
            } else if metric.starts_with("valor guardado") {
                declared_saved = val;
            } else if metric.starts_with("total de despesas") {
                declared_expense_total = val;
            }
            continue;
        }

        if cells.iter().all(|c| c.is_empty()) {
            continue;
        }
        if cells[0].eq_ignore_ascii_case("total") {
            let val_idx = hdr
                .iter()
                .position(|h| h.eq_ignore_ascii_case("Valor"))
                .unwrap_or(4);
            if let Some(v) = cells.get(val_idx).and_then(|s| parse_cents(s)) {
                if section == "expenses" && declared_expense_total.is_none() {
                    declared_expense_total = Some(v);
                } else if section == "incomes" && declared_income_total.is_none() {
                    declared_income_total = Some(v);
                }
            }
            continue;
        }

        let Some(mut dt) = parse_br_date(&cells[0]) else {
            continue;
        };
        let desc = cells.get(1).cloned().unwrap_or_default();
        let mut payment_raw = cells.get(2).cloned().unwrap_or_default();
        let mut card_raw = cells.get(3).cloned().unwrap_or_default();
        let amount_cents = cells.get(4).and_then(|s| parse_cents(s)).unwrap_or(0);
        let category_raw = cells.get(5).cloned().unwrap_or_default();
        let tags = cells.get(6).map(|s| parse_tags_cell(s)).unwrap_or_default();

        if section == "expenses" {
            // Fix swapped payment/card (e.g. expenses-2026-06 line 99: Forma="" Cartão="Pix")
            if payment_raw.is_empty()
                && matches!(
                    card_raw.to_lowercase().as_str(),
                    "pix" | "dinheiro" | "boleto" | "débito" | "debito" | "crédito" | "credito"
                )
            {
                payment_raw = card_raw;
                card_raw = String::new();
            }
            expenses.push(ParsedRow {
                date: dt,
                description: desc,
                payment_raw,
                card_or_dest_raw: card_raw,
                amount_cents,
                category_raw,
                tags,
            });
        } else if section == "incomes" {
            // Fix copied Proventos date (20/06/2026 in 2026-08 and 2026-09)
            if desc.eq_ignore_ascii_case("Proventos") {
                if let Ok((expected_start, _)) = cycle_bounds_for_key(cycle_key, 20) {
                    let diff_days = (expected_start - dt).whole_days().abs();
                    if diff_days > 20 {
                        dt = expected_start;
                    }
                }
            }
            incomes.push(ParsedRow {
                date: dt,
                description: desc,
                payment_raw,
                card_or_dest_raw: card_raw,
                amount_cents,
                category_raw,
                tags,
            });
        }
    }

    // Synthesize income for -02 and -03 where "Entradas e Receitas" section did not exist
    if incomes.is_empty() {
        if let Some(total_inc) = declared_income_total.or(salary_base) {
            if total_inc > 0 {
                let start_date = cycle_bounds_for_key(cycle_key, 20)
                    .map(|(s, _)| s)
                    .unwrap_or_else(|_| expenses.first().map(|e| e.date).unwrap());
                incomes.push(ParsedRow {
                    date: start_date,
                    description: "Proventos (sintetizado do resumo)".to_string(),
                    payment_raw: "Conta salário".to_string(),
                    card_or_dest_raw: "Banco do Brasil".to_string(),
                    amount_cents: total_inc,
                    category_raw: "Salário".to_string(),
                    tags: Vec::new(),
                });
            }
        }
    }

    ParsedFile {
        cycle_key: cycle_key.to_string(),
        title,
        declared_income_total,
        declared_expense_total,
        declared_saved,
        _salary_base: salary_base,
        expenses,
        incomes,
    }
}

fn ensure_account(
    conn: &mut SqliteConnection,
    cache: &mut HashMap<String, Account>,
    user_id: &str,
    name: &str,
    bank_name: &str,
    account_type: &str,
) -> String {
    let key = name.to_lowercase();
    if let Some(acc) = cache.get(&key) {
        return acc.id.clone();
    }
    let now = now_utc();
    let acc = Account {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        icon: None,
        balance: 0,
        bank_name: bank_name.to_string(),
        account_type: account_type.to_string(),
        user_id: user_id.to_string(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(accounts::table)
        .values(&acc)
        .execute(conn)
        .expect("insert account");
    let id = acc.id.clone();
    cache.insert(key, acc);
    id
}

fn ensure_category(
    conn: &mut SqliteConnection,
    cache: &mut HashMap<(String, String), Category>,
    user_id: &str,
    name: &str,
    cat_type: &str,
) -> String {
    let key = (name.to_lowercase(), cat_type.to_string());
    if let Some(cat) = cache.get(&key) {
        return cat.id.clone();
    }
    let now = now_utc();
    let cat = Category {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        description: None,
        color: Some("#607d8b".to_string()),
        icon: None,
        category_type: cat_type.to_string(),
        is_fixed: name.eq_ignore_ascii_case("Despesa fixa"),
        is_salary: name.eq_ignore_ascii_case("Salário"),
        user_id: user_id.to_string(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(categories::table)
        .values(&cat)
        .execute(conn)
        .expect("insert category");
    let id = cat.id.clone();
    cache.insert(key, cat);
    id
}

fn ensure_tag(
    conn: &mut SqliteConnection,
    cache: &mut HashMap<String, Tag>,
    user_id: &str,
    raw_name: &str,
) -> String {
    let key = raw_name.trim().trim_start_matches('#').to_lowercase();
    if let Some(t) = cache.get(&key) {
        return t.id.clone();
    }
    let now = now_utc();
    let t = Tag {
        id: Uuid::new_v4().to_string(),
        name: key.clone(),
        color: None,
        user_id: user_id.to_string(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(tags::table)
        .values(&t)
        .execute(conn)
        .expect("insert tag");
    let id = t.id.clone();
    cache.insert(key, t);
    id
}

fn main() {
    dotenv().ok();
    let args: Vec<String> = env::args().collect();
    let email = get_flag(&args, "--email")
        .unwrap_or_else(|| {
            eprintln!("Uso: import_vault --email <EMAIL> [--vault-dir <DIR>] [--dry-run] [--force]");
            std::process::exit(1);
        })
        .trim()
        .to_lowercase();
    let dry_run = has_flag(&args, "--dry-run");
    let force = has_flag(&args, "--force");

    let vault_dir = get_flag(&args, "--vault-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join("Documents/projects/vault/02-areas/finance/records")
        });

    let mut md_files: Vec<PathBuf> = fs::read_dir(&vault_dir)
        .unwrap_or_else(|e| panic!("Erro ao abrir vault dir {:?}: {e}", vault_dir))
        .filter_map(|res| res.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("expenses-") && n.ends_with(".md"))
                .unwrap_or(false)
        })
        .collect();
    md_files.sort();

    if md_files.is_empty() {
        eprintln!("Nenhum arquivo expenses-*.md encontrado em {:?}", vault_dir);
        std::process::exit(1);
    }

    let mut parsed_files = Vec::with_capacity(md_files.len());
    for path in &md_files {
        let fname = path.file_stem().unwrap().to_str().unwrap();
        let cycle_key = fname.strip_prefix("expenses-").unwrap().to_string();
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Erro lendo {:?}: {e}", path));
        let pf = parse_markdown_file(&cycle_key, &content);

        let sum_exp: i64 = pf.expenses.iter().map(|r| r.amount_cents).sum();
        let sum_inc: i64 = pf.incomes.iter().map(|r| r.amount_cents).sum();
        println!(
            "{} | {} | lanc={} soma_lanc={:.2} (decl={}) | entradas={} soma_ent={:.2} (decl={})",
            pf.cycle_key,
            pf.title,
            pf.expenses.len(),
            sum_exp as f64 / 100.0,
            pf.declared_expense_total
                .map(|v| format!("{:.2}", v as f64 / 100.0))
                .unwrap_or_else(|| "-".into()),
            pf.incomes.len(),
            sum_inc as f64 / 100.0,
            pf.declared_income_total
                .map(|v| format!("{:.2}", v as f64 / 100.0))
                .unwrap_or_else(|| "-".into()),
        );

        if let Some(decl_exp) = pf.declared_expense_total {
            assert_eq!(
                sum_exp, decl_exp,
                "Divergência de despesas em {}: calculado={} vs declarado={}",
                pf.cycle_key, sum_exp, decl_exp
            );
        }
        if let Some(decl_inc) = pf.declared_income_total {
            assert_eq!(
                sum_inc, decl_inc,
                "Divergência de receitas em {}: calculado={} vs declarado={}",
                pf.cycle_key, sum_inc, decl_inc
            );
        }
        parsed_files.push(pf);
    }

    if dry_run {
        println!("Dry-run concluído: todos os arquivos reconciliados com 0 centavos de diferença.");
        return;
    }

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = build_pool(&database_url);
    let mut conn = pool.get().expect("DB connection");

    let user = users::table
        .filter(users::email.eq(&email))
        .filter(users::deleted_at.is_null())
        .first::<User>(&mut conn)
        .unwrap_or_else(|_| {
            eprintln!("Usuário ativo não encontrado: {email}");
            std::process::exit(1);
        });

    let existing_tx_count: i64 = transactions::table
        .filter(transactions::user_id.eq(&user.id))
        .filter(transactions::deleted_at.is_null())
        .count()
        .get_result(&mut conn)
        .unwrap_or(0);
    if existing_tx_count > 0 && !force {
        eprintln!(
            "Usuário já possui {existing_tx_count} transações. Use --force se quiser importar mesmo assim."
        );
        std::process::exit(1);
    }

    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        seed_user_defaults(conn, &user.id)?;

        let mut acc_cache: HashMap<String, Account> = accounts::table
            .filter(accounts::user_id.eq(&user.id))
            .filter(accounts::deleted_at.is_null())
            .load::<Account>(conn)?
            .into_iter()
            .map(|a| (a.name.to_lowercase(), a))
            .collect();

        let mut cat_cache: HashMap<(String, String), Category> = categories::table
            .filter(categories::user_id.eq(&user.id))
            .filter(categories::deleted_at.is_null())
            .load::<Category>(conn)?
            .into_iter()
            .map(|c| ((c.name.to_lowercase(), c.category_type.clone()), c))
            .collect();

        let mut tag_cache: HashMap<String, Tag> = tags::table
            .filter(tags::user_id.eq(&user.id))
            .filter(tags::deleted_at.is_null())
            .load::<Tag>(conn)?
            .into_iter()
            .map(|t| (t.name.to_lowercase(), t))
            .collect();

        let bb_id = ensure_account(conn, &mut acc_cache, &user.id, "Banco do Brasil", "Banco do Brasil", "checking");
        let inv_id = ensure_account(conn, &mut acc_cache, &user.id, "Investimentos", "Corretora", "investment");
        let cash_id = ensure_account(conn, &mut acc_cache, &user.id, "Carteira", "Dinheiro", "cash");

        let now = now_utc();
        let mut balance_deltas: HashMap<String, i64> = HashMap::new();
        let mut imported_txs = 0usize;
        let mut imported_transfers = 0usize;

        for pf in &parsed_files {
            // Compute non-overlapping cycle bounds around user's cycle_start_day or file dates
            let (bound_s, bound_e) = cycle_bounds_for_key(&pf.cycle_key, user.cycle_start_day)
                .unwrap_or_else(|_| (Date::MIN, Date::MAX));
            let label = if !pf.title.is_empty() {
                format!("{} ({})", pf.title, default_cycle_label(bound_s, bound_e))
            } else {
                default_cycle_label(bound_s, bound_e)
            };

            let existing_cfg: Option<CycleConfig> = cycle_configs::table
                .filter(cycle_configs::user_id.eq(&user.id))
                .filter(cycle_configs::cycle_key.eq(&pf.cycle_key))
                .first(conn)
                .optional()?;

            if existing_cfg.is_none() {
                let saved_cents = pf.declared_saved.unwrap_or(0);
                let cfg = CycleConfig {
                    id: Uuid::new_v4().to_string(),
                    user_id: user.id.clone(),
                    cycle_key: pf.cycle_key.clone(),
                    start_date: format_ymd(bound_s),
                    end_date: format_ymd(bound_e),
                    label: Some(label),
                    savings_mode: "fixed".to_string(),
                    savings_percent: None,
                    savings_fixed_amount: Some(saved_cents),
                    notes: None,
                    created_at: now,
                    updated_at: now,
                };
                diesel::insert_into(cycle_configs::table)
                    .values(&cfg)
                    .execute(conn)?;
            }

            // Import incomes
            for row in &pf.incomes {
                let pm = map_payment_method(&row.payment_raw, true);
                let acc_name = normalize_card_name(&row.card_or_dest_raw)
                    .unwrap_or_else(|| "Banco do Brasil".to_string());
                let acc_id = if acc_name.eq_ignore_ascii_case("Banco do Brasil") {
                    bb_id.clone()
                } else if pm == "cash" {
                    cash_id.clone()
                } else {
                    ensure_account(conn, &mut acc_cache, &user.id, &acc_name, &acc_name, "checking")
                };
                let cat_name = normalize_category_name(&row.category_raw);
                let cat_id = ensure_category(conn, &mut cat_cache, &user.id, &cat_name, "income");
                let dt = PrimitiveDateTime::new(row.date, Time::from_hms(12, 0, 0).unwrap());

                let tx = Transaction {
                    id: Uuid::new_v4().to_string(),
                    amount: row.amount_cents,
                    description: Some(row.description.clone()),
                    date: dt,
                    type_: Type::Income,
                    payment_method: pm.to_string(),
                    card_name: None,
                    account_id: acc_id.clone(),
                    transfer_id: None,
                    category_id: Some(cat_id),
                    is_fixed: None,
                    is_informational: false,
                    credit_paid: false,
                    installment_group_id: None,
                    installment_number: None,
                    installment_total: None,
                    recurring_rule_id: None,
                    needs_confirmation: false,
                    user_id: user.id.clone(),
                    deleted_at: None,
                    created_at: now,
                    updated_at: now,
                };
                diesel::insert_into(transactions::table)
                    .values(&tx)
                    .execute(conn)?;
                for tname in &row.tags {
                    let tid = ensure_tag(conn, &mut tag_cache, &user.id, tname);
                    diesel::insert_into(transaction_tags::table)
                        .values(TransactionTag {
                            transaction_id: tx.id.clone(),
                            tag_id: tid,
                        })
                        .execute(conn)?;
                }
                *balance_deltas.entry(acc_id).or_insert(0) += row.amount_cents;
                imported_txs += 1;
            }

            // Import expenses & investment transfers
            for row in &pf.expenses {
                let cat_name = normalize_category_name(&row.category_raw);
                let dt = PrimitiveDateTime::new(row.date, Time::from_hms(12, 0, 0).unwrap());

                if cat_name.eq_ignore_ascii_case("Investimentos") {
                    let tr = AccountTransfer {
                        id: Uuid::new_v4().to_string(),
                        from_account_id: bb_id.clone(),
                        to_account_id: Some(inv_id.clone()),
                        amount: row.amount_cents,
                        description: Some(row.description.clone()),
                        date: dt,
                        transfer_kind: "investment".to_string(),
                        user_id: user.id.clone(),
                        deleted_at: None,
                        created_at: now,
                        updated_at: now,
                    };
                    diesel::insert_into(account_transfers::table)
                        .values(&tr)
                        .execute(conn)?;
                    for tname in &row.tags {
                        let tid = ensure_tag(conn, &mut tag_cache, &user.id, tname);
                        diesel::insert_into(transfer_tags::table)
                            .values(TransferTag {
                                transfer_id: tr.id.clone(),
                                tag_id: tid,
                            })
                            .execute(conn)?;
                    }
                    *balance_deltas.entry(bb_id.clone()).or_insert(0) -= row.amount_cents;
                    *balance_deltas.entry(inv_id.clone()).or_insert(0) += row.amount_cents;
                    imported_transfers += 1;
                    continue;
                }

                let pm = map_payment_method(&row.payment_raw, false);
                let card_name = normalize_card_name(&row.card_or_dest_raw);
                let acc_id = if let Some(ref cname) = card_name {
                    ensure_account(conn, &mut acc_cache, &user.id, cname, cname, "checking")
                } else if pm == "cash" {
                    cash_id.clone()
                } else {
                    bb_id.clone()
                };

                let cat_id = ensure_category(conn, &mut cat_cache, &user.id, &cat_name, "expense");
                let is_info = row.amount_cents == 0 && pm == "credit";

                let tx = Transaction {
                    id: Uuid::new_v4().to_string(),
                    amount: row.amount_cents,
                    description: Some(row.description.clone()),
                    date: dt,
                    type_: Type::Expense,
                    payment_method: pm.to_string(),
                    card_name,
                    account_id: acc_id.clone(),
                    transfer_id: None,
                    category_id: Some(cat_id),
                    is_fixed: None,
                    is_informational: is_info,
                    credit_paid: false,
                    installment_group_id: None,
                    installment_number: None,
                    installment_total: None,
                    recurring_rule_id: None,
                    needs_confirmation: false,
                    user_id: user.id.clone(),
                    deleted_at: None,
                    created_at: now,
                    updated_at: now,
                };
                diesel::insert_into(transactions::table)
                    .values(&tx)
                    .execute(conn)?;
                for tname in &row.tags {
                    let tid = ensure_tag(conn, &mut tag_cache, &user.id, tname);
                    diesel::insert_into(transaction_tags::table)
                        .values(TransactionTag {
                            transaction_id: tx.id.clone(),
                            tag_id: tid,
                        })
                        .execute(conn)?;
                }

                let delta = transaction_balance_delta(&tx.type_, &tx.payment_method, tx.is_informational, tx.amount);
                *balance_deltas.entry(acc_id).or_insert(0) += delta;
                imported_txs += 1;
            }
        }

        for (aid, delta) in balance_deltas {
            if delta != 0 {
                let acc = accounts::table
                    .filter(accounts::id.eq(&aid))
                    .first::<Account>(conn)?;
                diesel::update(accounts::table.filter(accounts::id.eq(&aid)))
                    .set((
                        accounts::balance.eq(acc.balance + delta),
                        accounts::updated_at.eq(now),
                    ))
                    .execute(conn)?;
            }
        }

        println!("Importação concluída: {imported_txs} transações e {imported_transfers} transferências de investimento.");
        Ok(())
    })
    .expect("Erro durante importação no banco");
}
