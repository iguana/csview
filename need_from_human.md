# Things needed from the human

## Integration polish needed (no human input required)

The csviewai app launches and renders the full chrome (titlebar, toolbar, grid headers, AI menu bar with all 11 feature items). Two integration issues remain from the frontend expecting csview's API shape:
- Grid data cells don't render because `RangeCache` calls `readRange(fileId, start, end)` but csviewai's API uses `readRange(fileId, offset, limit, orderBy)`. Fix: update the AI `App.tsx` to construct the cache with the correct fetcher signature.
- "sort_csv not found" error because the shared DataGrid tries to call the csview sort command. Fix: add a `sort_csv` command to csviewai that wraps `SqliteStore::read_range` with an ORDER BY clause.

These are straightforward adapter-layer fixes, not missing features.

## Before testing AI features end-to-end

1. **Anthropic API key** — The app prompts users for their API key in the Account panel at runtime. For development/testing of the LLM integration, I need either:
   - An `ANTHROPIC_API_KEY` environment variable set in the shell, OR
   - A test key I can hardcode into integration test fixtures
   - **Status**: Not yet needed — unit tests use mock LLM responses. Only needed for manual end-to-end testing.

2. **Model preference** — Currently defaulting to `claude-sonnet-4-20250514` for cost/quality balance. Alternatives:
   - `claude-sonnet-4-20250514` — faster, cheaper, good for AST generation
   - `claude-opus-4-20250514` — slower, more expensive, better for complex reasoning
   - **Status**: Using Sonnet as default. Can be changed in Account settings.

3. **Token budget per call** — Current defaults:
   - AST generation (filter/transform): 512 max_tokens
   - Narrative generation (profiles, explanations): 2048 max_tokens
   - Chat responses: 1024 max_tokens
   - Report generation: 4096 max_tokens
   - **Status**: Hardcoded defaults, adjustable in the prompt templates.

## Before publishing

4. **App signing certificate** — The DMG is unsigned. For notarization (removing the right-click-to-open requirement), need an Apple Developer certificate.
   - **Status**: Not blocking. Users can work around with right-click → Open.

5. **Pricing** — The plan mentions free (csview) vs paid (csviewai) tiers. No pricing or billing is implemented. The current architecture just requires an API key.
   - **Status**: Not blocking for development. Pricing/billing can be added later.

## Follow-ups

- **Move API-key storage out of SQLite into the macOS Keychain** via
  `tauri-plugin-keyring`. Today the key sits unencrypted at
  `~/Library/Application Support/dev.csview.ai/csviewai.db` in the
  `account` table (`api_key` column stores `"{provider}:{raw_key}"`).
  The plugin would scope the secret to the user account / app and avoid
  surfacing it in disk backups, sync drives, or process inspections.
  Migration: on first launch, if a row exists in `account`, copy the key
  into the keychain and clear the column.
