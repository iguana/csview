# Things needed from the human

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
