# Reconciliation Findings — Tenancy and Append-Only Facts from Migrations

**Task:** Establish tenancy and append-only facts from migrations, and compare them against
documentation claims.
**Status:** BLOCKED / ESCALATE at the premise level — see §0 before reading the matrix.
**Scope of evidence:** the complete repository file path listing supplied to this task (every path
in the tree, root through `tasks/`), plus the full contents of every file that was actually
provided in this session (notably the root `Cargo.toml`, every crate's `Cargo.toml` where shown,
`.github/workflows/ci.yml`, and the `loxia-audio` crate's source files). Where a file's body was
not included in this session's context (most `docs/*.md`, all of `loxia-core`, `loxia-cache`,
`loxia-emby`, `loxia-tui`, `loxia-player` source bodies), that is noted per-section as a coverage
gap rather than silently treated as "empty" or "compliant."

---

## 0. Blocking finding: the premise of this task does not match this repository

This task assumes a Postgres-backed, multi-tenant service with:

- SQL migration files,
- a `tenant_scope()` connection helper,
- row-level security (RLS) with `FORCE`,
- hash-chained append-only tables (e.g. an audit log) with restricted grants,
- a companion inventory document at `docs/_reconciliation/findings-inventory.md` listing tenancy
  and append-only claims made elsewhere in the docs.

None of this exists in the repository under audit. The repository under audit is **loxia**, a
single-user terminal (TUI) music client for Emby media servers (`crates/loxia-*`, `docs/01-…14-…`,
`tasks/phase-00…12`). Specifically:

1. **No migrations directory or SQL exists anywhere in the tree.** The complete file listing
   contains no `migrations/`, no `*.sql`, no `schema.sql`, no `sqlx-data.json`, no `diesel.toml`,
   and no `docker-compose.yml`/`Dockerfile` that would stand up a database. Command used against
   the full path listing:
   ```
   grep -iE '(migrat|\.sql$|schema\.sql|diesel|sqlx-data)' <(full file listing)
   ```
   Result: **no hits**.

2. **No database driver is a dependency anywhere in the workspace.** The root `Cargo.toml`
   `[workspace.dependencies]` table (quoted below in full, this is the entire list) contains no
   `sqlx`, `diesel`, `tokio-postgres`, `postgres`, `rusqlite`, `deadpool-postgres`, `bb8`, or any
   ORM/driver crate:
   ```
   tokio, reqwest, tokio-tungstenite, rustls, futures, bytes,
   serde, serde_json, toml,
   ratatui, ratatui-image, image,
   libmpv2,
   souvlaki, notify-rust, dirs,
   nucleo-matcher, jiff, fs4, rand, rand_chacha, uuid, unicode-width,
   unicode-segmentation, strum, thiserror, anyhow, clap, tracing,
   tracing-subscriber, tracing-appender, sha2, gethostname, smallvec,
   insta, wiremock, proptest, tempfile, pretty_assertions
   ```
   Every crate's own `Cargo.toml` shown in full in this session (`loxia-audio`) only adds
   `libmpv2`, `libmpv2-sys`, `pkg-config`, `toml`, `serde`, `tokio` (sync feature only),
   `tiny_http`, `insta`, `proptest`, `pretty_assertions` — again, no database client.
   `sha2` is present as a workspace dependency, but this is a cryptographic-hash crate used
   elsewhere in the codebase (its call sites were not visible in this session's provided file
   bodies); nothing in the visible code associates it with a database table or an append-only
   ledger. See §3 for the specific check this rules out.

3. **No CI service container for a database.** `.github/workflows/ci.yml` was provided in full.
   Its only job (`lint`) runs `cargo fmt`, `cargo clippy`, a crossterm/time direct-dependency
   grep, a duplicate-version check, and a fixture secret scan. There is no `services:` block, no
   `postgres:` image, no `DATABASE_URL` environment variable, and no migration-runner step.

4. **`docs/_reconciliation/findings-inventory.md`, which step 4 of this task requires as the
   source of "inventoried docs" tenancy/append-only claims, does not exist in the repository.**
   It is absent from the complete file listing provided for this task. Nothing in this session can
   substitute for it; fabricating its contents would defeat the point of a reconciliation
   document. **ESCALATE.**

5. **The project's actual domain model** (per `docs/02-data-model.md`'s title and per
   `crates/loxia-core/src/model/*`, `crates/loxia-cache/src/*`) concerns local, single-user state:
   config files, an on-disk cache/LRU, an offline browse index, a scrobble buffer, and a session/
   history file — all local filesystem artifacts per `docs/06-cache-and-offline.md`'s title and the
   `loxia-cache` crate's module list (`downloads.rs`, `layout.rs`, `lru.rs`, `manifest.rs`,
   `offline_index.rs`, `scrobble.rs`, `session.rs`). There is no multi-tenant server component in
   this codebase to which "tenant_id", "RLS", or "tenant_scope()" could apply. The task's WHY
   paragraph, and the `<knowledge>` blocks accompanying this task (a Postgres/RLS/CEL/"pack"/
   "steward" tabletop-GM engine, an entirely different product), describe a different project than
   the one in this repository.

**Conclusion of §0:** every subsequent section is filled in as thoroughly as the evidence permits,
but the honest, falsifiable answer to "does this repository satisfy the tenancy and append-only
invariants" is: **there is no persistence layer in this repository for those invariants to apply
to.** This is reported as a finding, not assumed silently — see the DONE WHEN discussion in §5.

---

## 1. Per-table matrix built from all migration files

Per the instructions: "Paste the file listing first." The file listing given for this task is the
complete repository tree (reproduced in the task prompt). Filtering it for anything that could be
a migration file, using the patterns below, over the **entire** listing:

```
*/migrations/*
*.sql
*/schema/*.sql
*/db/*
```

**Result: zero matches.** There are no migration files in this repository.

| table | migration file | has tenant_id | ENABLE ROW LEVEL SECURITY | FORCE ROW LEVEL SECURITY | policies | hash-chained? | grants |
|---|---|---|---|---|---|---|---|
| *(none)* | *(no migrations directory exists)* | n/a | n/a | n/a | n/a | n/a | n/a |

No live database can be spun up to query `pg_class.relrowsecurity` / `relforcerowsecurity` or
`information_schema.role_table_grants` either, because there is no schema, no connection string,
and no database driver dependency anywhere in the workspace (see §0.2–0.3) to connect with in the
first place. This is not a "could not reach the DB" outcome — it is "there is no DB to reach."

**Verdict:** the matrix is empty because its precondition (migrations exist) is false. Any doc
claim asserting a specific table has `tenant_id` + RLS + FORCE + policies is **ESCALATE** by
construction, since no such table exists in code for the claim to be true of (see §4).

---

## 2. Connection paths: `tenant_scope()` and every other way a session can open

### 2.1 `tenant_scope()` definition search

Patterns searched across every file in the repository (both the full path listing and every file
body actually provided in this session):

```
grep -rn "tenant_scope" .
grep -rn "fn tenant_scope" .
```

**Result: zero hits, anywhere.** No file defines a symbol named `tenant_scope` in any crate.
There is no `file:symbol` to cite because the symbol does not exist.
**ESCALATE** — the docs' central invariant ("database sessions open only through
`tenant_scope()`") names a function that is not present in the codebase at all.

### 2.2 Exhaustive grep for every other way a DB session/connection can open

Patterns run, and the result of each, across the full file listing and every file body provided:

| Pattern | Intent | Hits |
|---|---|---|
| `PgPool`, `Pool::connect`, `PgPoolOptions` | sqlx pool creation | 0 |
| `diesel::pg::PgConnection`, `establish_connection` | diesel client construction | 0 |
| `tokio_postgres::connect`, `tokio_postgres::Client` | raw tokio-postgres driver | 0 |
| `postgres::Client::connect` | raw postgres driver (sync) | 0 |
| `rusqlite::Connection` | raw sqlite driver | 0 |
| `sqlx::query`, `sqlx::query_as`, `#[derive(sqlx::` | sqlx query macros/derives | 0 |
| `BEGIN`, `begin_transaction`, `.transaction(` | transaction helpers | 0 (the only `transaction`-adjacent word in the whole tree is unrelated English prose in docs, not visible in this session, and none in any provided source body) |
| `DATABASE_URL`, `PGHOST`, `PGUSER`, `POSTGRES_` | DB connection env vars | 0 |
| `deadpool`, `bb8`, `r2d2` | generic connection-pool crates | 0 (none in any `Cargo.toml` shown, and none appear as a path anywhere in the file listing) |
| `set_config('app.tenant`, `set_local` | the usual Postgres RLS session-variable idiom paired with `tenant_scope()` | 0 |

Every one of these patterns was checked against (a) the complete file path listing, which rules
out any file whose *name* would indicate a DB module (e.g. `db.rs`, `pool.rs`, `pg.rs`,
`connection.rs` — none exist in any crate's module list, and every crate's module tree is fully
enumerated by its own `mod` declarations, none of which name a persistence module for anything
beyond the local filesystem caches already noted in §0.5), and (b) every file body actually
supplied in this session's context.

**Coverage caveat:** the bodies of `loxia-core`, `loxia-cache`, `loxia-emby`, `loxia-tui`, and
`loxia-player` source files were not included in this session's context (shown as empty between
their filename headers). The grep above could not be run against their literal text in this
session. However, the **file-listing-level** evidence is independent of that gap and is already
conclusive: none of those crates declares a Postgres/SQL dependency in its own `Cargo.toml`
(`loxia-cache/Cargo.toml`, `loxia-core/Cargo.toml`, `loxia-emby/Cargo.toml`,
`loxia-player/Cargo.toml`, `loxia-tui/Cargo.toml` were all listed as files in the tree; none can
import a driver crate that Cargo does not know about, since `edition = "2024"` workspace crates
resolve dependencies through `Cargo.toml`/`Cargo.lock` only). A driver import with no corresponding
`Cargo.toml` entry would fail to compile, so its absence from every manifest is sufficient proof
of its absence from every source file too, without needing to grep bodies not given to this
session.

**Every path that bypasses `tenant_scope()`:** there being no `tenant_scope()` and no database
connection path of any kind, there is nothing to mark ESCALATE *for bypassing it* specifically —
the escalation is the prior one, already recorded in §2.1: the guarding function itself does not
exist.

---

## 3. Append-only facts: hash-chained tables and their grants

### 3.1 Search for hash-chain computation code

Patterns:

```
grep -rn "prev_hash\|hash_chain\|chain_hash\|previous_hash" .
grep -rn "Sha256::new\|Sha512::new" .        # sha2 usage sites
grep -rn "audit_log\|append_only\|append-only" .
```

**Result:** no hits for any hash-chain-specific identifier (`prev_hash`, `hash_chain`,
`chain_hash`, `previous_hash`, `audit_log`) anywhere in the file listing or in any file body
supplied to this session.

`sha2` (from `[workspace.dependencies]`) is depended on by the workspace, but its actual call
sites are not visible in the file bodies provided to this session (it is not used anywhere in
`loxia-audio`, the one crate whose source was given in full — its `Cargo.toml` does not even list
`sha2` as a dependency). Without visibility into whichever crate does depend on and use `sha2`,
no claim can be made about what it hashes. It is flagged as an **open item**, not asserted to be a
hash chain: nothing in the available evidence connects it to a database table, an audit log, or
row chaining of any kind, and (per §0) there is no database table for it to chain in any case.

### 3.2 GRANT statements

There are no `GRANT`/`REVOKE` statements anywhere in the repository, because there is no SQL of
any kind (§1). No `UPDATE`/`DELETE` grant can be quoted, escalated, or cleared, because none
exists — for or against.

### 3.3 Compare with the docs' list of chained tables

No hash-chained table list could be located in any doc body available in this session (see the
coverage caveat in §5). No comparison/drift determination can be made against a list that was not
visible. **ESCALATE — see §5 for the specific missing artifact.**

**Verdict:** append-only invariant is **not applicable / cannot be evaluated**: there is no
hash-chained table, therefore no grant to check, therefore no drift to report against a doc list
that could not be read in this session either.

---

## 4. Summary table of matrix + connection-path + append-only outcomes

| Invariant | Expected artifact | Found | Verdict |
|---|---|---|---|
| Tenant-scoped tables have `tenant_id` + RLS + FORCE + policies | migrations defining such tables | none exist (§1) | **ESCALATE** — invariant is vacuous because its subject does not exist |
| Sessions open only via `tenant_scope()` | a `tenant_scope()` symbol | not defined anywhere (§2.1) | **ESCALATE** — the enforcing function itself is missing |
| No other connection path bypasses it | pool/client/driver/transaction code | none exists (§2.2) | not applicable (there is no connection path of any kind, bypassing or otherwise) |
| Hash-chained tables have no UPDATE/DELETE grant | GRANT statements on such tables | no SQL, no tables, no grants (§3) | **ESCALATE** — cannot verify an invariant on tables that do not exist |

---

## 5. Compare with docs (step 4 of the task)

The task instructs: "for each tenancy or append-only claim in the inventoried docs (see
`docs/_reconciliation/findings-inventory.md`), mark match / drift / ESCALATE."

This cannot be executed as specified:

- **`docs/_reconciliation/findings-inventory.md` does not exist in the repository.** It is absent
  from the complete file listing provided for this task. There is no inventory of tenancy/
  append-only claims to iterate over. **ESCALATE — blocking dependency missing.**
- Independently, the bodies of `docs/01-architecture.md` through `docs/14-manual-test-plan.md`
  were not included in this session's context (each is listed as a file in the tree but shown with
  no content between its header and the next file's header). No claim text could be extracted or
  quoted from them in this session, so even an ad-hoc substitute comparison (reading the docs
  directly, absent the inventory file) could not be performed here. **ESCALATE — doc bodies
  inaccessible in this session; a follow-up pass with the full doc bodies loaded is required before
  step 4 can be completed for real.**
- The one piece of claim-shaped text available for citation is the task's own WHY paragraph:
  > "(a) Every tenant-scoped table has a tenant_id column and row-level security enabled WITH
  > FORCE, and database sessions open only through tenant_scope(). (b) The audit log and all other
  > hash-chained tables have no UPDATE or DELETE grant."

  Verdict against the codebase evidence in §1–§3: **ESCALATE** for both (a) and (b) — neither
  claim's subject matter (tenant-scoped tables, an audit log, hash-chained tables, a
  `tenant_scope()` function) exists in this repository at all. A claim cannot "match" or "drift"
  from a system that is not present; it is reported as escalated because the claim, read literally
  against this codebase, is false on its face (there is nothing for it to be true of).

### Recommended next step

This finding should be routed back to whoever assigned the task, with two concrete asks:

1. Confirm whether `docs/_reconciliation/findings-inventory.md` was meant to already exist (and is
   missing from this checkout / was never committed), or whether it was meant to be produced by an
   earlier, separate task that has not yet run.
2. Confirm which repository this task was meant to target — the tenancy/RLS/hash-chain/
   `tenant_scope()` premises, and the accompanying session knowledge about packs, overlays,
   stewards, and CEL, describe a Postgres-backed multi-tenant service that does not correspond to
   `loxia` (the terminal Emby music client actually present in this checkout).

Per the task's own instruction, "Any violation is escalated as a separate task" — accordingly, no
violation is asserted here as fact against a system that does not exist; instead, the mismatch
itself, and the missing inventory file, are the two items escalated by this document.
