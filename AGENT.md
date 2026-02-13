# AGENTS.md — Coding Agent Instructions for Momentum

This file defines how a coding agent should work in the Momentum repository.
Follow it strictly to avoid architectural drift, broken builds, or UX regressions.

---

## 1. Project overview

**Momentum** is a desktop screen recording app built with:

- **Backend**: Rust + Tauri (v2)
- **Frontend**: React 19 + TypeScript
- **State management**: Zustand
- **Styling**: Tailwind CSS
- **Icons**: Lucide
- **Target platform**: macOS first (Windows/Linux later)

The app is a **minimal always-on-top overlay** for screen recording, with optional
camera and microphone capture using **native OS APIs**.

---

## 2. Sources of truth (priority order)

When making decisions, follow this order **strictly**:

1. `ROADMAP.md`
2. `PRODUCT_SPECS.md`
3. `AGENTS.md` (this file)
4. Existing codebase
5. General knowledge

If the codebase contradicts specs or roadmap:

- **Call it out explicitly**
- Propose a reconciliation
- Do not silently “fix” behavior

If something is unclear:

- Make a reasonable assumption
- State it explicitly
- Continue

---

## 3. Known-good commands

Prefer these exact commands.

### Frontend

- Dev: `npm run dev`
- Build: `npm run build`
- Preview build: `npm run preview`

### Tauri

- Run app: `npm run tauri dev`
- Build app: `npm run tauri build`

Do **not** invent new scripts unless necessary.
If you add one, document it here.

---

## 4. Repository structure

### Frontend (`src/`)

- `src/pages/` — route-level components
- `src/components/` — shared UI components
- `src/features/<featureName>/`
  - `components/`
  - `hooks/`
  - `api.ts`
  - `types.ts`
- `src/hooks/` — reusable hooks
- `src/api/` — Tauri command wrappers (no raw `invoke` in components)
- `src/types/` — shared TypeScript types
- `src/lib/` or `src/utils/` — helpers

**Rule**: new behavior goes into a **feature folder**, not random components.

---

### Backend (`src-tauri/`)

- `src-tauri/src/main.rs` — Tauri bootstrap
- `src-tauri/src/commands/` — `#[tauri::command]` functions
- `src-tauri/src/services/` — business / domain logic
- `src-tauri/src/models/` — DTOs and shared structs
- `src-tauri/src/error.rs` or `errors.rs` — central error types

**Rules**:

- Commands are thin (validation + orchestration only)
- Real logic lives in `services/`
- Errors are centralized and mapped at boundaries

---

## 5. Tauri ↔ React contract

### Rust side

- Use `#[tauri::command]`
- Strongly typed params and returns
- Return `Result<T, AppError>`
- Never panic in production paths
- No arbitrary shell execution
- File system access must be explicit and justified

### Frontend side

- Wrap Tauri commands in `src/api/` or feature-level `api.ts`
- Do **not** scatter raw `invoke` calls
- Mirror Rust DTOs in TypeScript
- Update Rust + TS types together in the same change

---

## 6. Coding conventions

### General

- Favor clarity over cleverness
- Descriptive names
- Pure functions when possible
- Comment **why**, not **what**

---

### Rust

- Rust 2021 edition
- Use `Result` + `?`
- Avoid `unwrap()` / `expect()` unless explicitly justified
- Small focused modules
- Derive traits (`Debug`, `Clone`, `Serialize`, `Deserialize`) when useful

**Error handling**

- Use a central error enum (via `thiserror`)
- Map errors to user-facing messages at command boundaries
- Do not leak low-level details to the UI

---

### React / TypeScript

- TypeScript only (no JS)
- Functional components + hooks only
- Avoid `any`
- One responsibility per component
- Extract reusable logic into hooks

**Async behavior**

- Always represent loading state
- Handle errors explicitly
- Never leave UI in ambiguous state

---

## 7. UI & UX constraints (important)

The UI is a **frameless, always-on-top overlay**.

Key constraints:

- Minimal, distraction-free
- Small surface area
- Clear recording state at all times

If behavior affects:

- Recording state
- Timing
- Visibility
- Camera/mic/system audio toggles

→ double-check against `PRODUCT_SPECS.md`

Screenshots and visual references live in `/_AGENTS/images/`.

---

## 8. Performance expectations

- Do not block the UI thread
- Long operations must show feedback
- Prefer async Rust tasks where applicable
- Avoid heavy computation in React render
- Use memoization only when justified

---

## 9. Security & privacy

Assume **sensitive user data**.

Do NOT:

- Log recordings, file paths, or sensitive metadata
- Hardcode secrets or keys
- Expand Tauri permissions casually
- Run arbitrary shell commands

If something is risky:

- Say so explicitly
- Offer a safer alternative

---

## 10. Testing

### Rust

- Unit tests in `#[cfg(test)]` modules
- Focus on services / logic
- Behavior over implementation

### React (if test infra exists)

- Test critical flows
- Test non-trivial hooks
- Prefer Testing Library patterns

Do not add heavy test infra for small changes.
Suggest tests when logic is complex.

---

## 11. How to respond to tasks

When implementing changes:

1. **Understand**

   - Read relevant specs and roadmap
   - Scan existing code for patterns

2. **Plan briefly**

   - 1–3 bullet points if non-trivial

3. **Implement**

   - Minimal, coherent changes
   - One feature at a time

4. **Output**

   - Code first, explanation second
   - One full code block per file
   - Include imports and types

5. **Explain briefly**
   - What changed
   - Why this approach
   - Tradeoffs or follow-ups

---

## 12. Hard rules (do not break)

- Do not silently change public behavior
- Do not introduce major dependencies lightly
- Do not ignore TypeScript errors
- Do not alter Tauri config without calling it out
- Do not invent APIs without marking them **to be implemented**

When unsure:
→ Choose the simplest reasonable solution  
→ State assumptions  
→ Move forward
