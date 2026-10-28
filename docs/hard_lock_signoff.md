# Hard-lock unlock sign-off packet

> **Status:** Decision input only (Phase 59 Wave R).  
> **Rule:** No implementation wave may start on a locked item until the product and engineering checkboxes below are marked for that item. An unsigned packet is not approval to ship.

Rust applications use `synui::prelude::*`. Non-Rust hosts use Production ABI `synui-abi` v1.1. Unlocking a hard lock does **not** imply an ABI bump unless a separate ABI sign-off is attached.

Hard locks originate from Phase 24/25+ Non-goals in [`DEVELOPMENT.md`](DEVELOPMENT.md) and the gap matrix in [`examples_facade_gap_and_next_steps.md`](examples_facade_gap_and_next_steps.md).

## Recommended unlock order

1. **Style Cascade v3** — smallest increment on Phase 40 Cascade v2; unlocks `style_demo` depth.
2. **Floating dock** — largest demo gap (`data_browser_demo`); larger engineering surface.
3. **RichEdit OT / HTML** — pick **one** MVP axis after sign-off; unlocks `rich_*`.
4. **GTK true menus** — Linux host shell only; stays outside prelude.

---

## 1. Style Cascade v3

| Field | Content |
|-------|---------|
| Motivation | Cascade v2 covers class specificity / `!important` / descendant; demos still lack id selectors and limited `@media`. |
| Suggested MVP (only after sign-off) | `#id` selectors; limited `@media` (width breakpoints); keep existing class / descendant / `!important` behavior. |
| Explicitly still out | Full container queries; universal selector wars; restoring panic CSS paths. |
| Dependencies | Phase 40 Style Cascade v2 + StyleCache; prelude `Styled`. |
| Demo value | `style_demo` depth beyond current grid/flex/absolute MVP. |
| Risks | Specificity regressions; theme flip + media re-resolve cost. |
| Production ABI | No change unless a later ABI wave exports style APIs (not requested). |

**Sign-off**

- [ ] Product: approve Cascade v3 MVP scope above
- [ ] Engineering: capacity and regression plan approved
- [ ] Implementation wave id / date: ________

---

## 2. Floating dock

| Field | Content |
|-------|---------|
| Motivation | `data_browser_demo` still cannot tear/redock panes; only fixed `SplitPane` trees. |
| Suggested MVP (only after sign-off) | Single-window dock: detach/reattach Split+Tab panes within one `Window`; persist layout in App `State`. |
| Explicitly still out | Cross-window / cross-process panes; tear to OS desktop; muda-only chrome. |
| Dependencies | Prelude `SplitPane` / `TabBar`; overlay hit-testing; no gold-archive restore. |
| Demo value | `data_browser_demo` parity vs historical dock UX. |
| Risks | Focus/overlay stacking; layout thrash; a11y tree for floating hosts. |
| Production ABI | No change for MVP (Rust facade only). |

**Sign-off**

- [ ] Product: approve floating dock MVP scope above
- [ ] Engineering: capacity and regression plan approved
- [ ] Implementation wave id / date: ________

---

## 3. RichEdit OT / HTML

| Field | Content |
|-------|---------|
| Motivation | `rich_edit_demo` / `rich_blocks_demo` lack interchange and multi-user edit paths. |
| Suggested MVP (only after sign-off) | **Choose exactly one:** (A) HTML round-trip import/export for `RichDocument`, **or** (B) single-document OT against one peer/protocol. |
| Explicitly still out | CRDT; nested editable tables; full Word/Google Docs parity. |
| Dependencies | Prelude `RichDocument` / `BlockDoc`; existing MD export. |
| Demo value | `rich_*` demos gain one production-shaped interchange or collab axis. |
| Risks | Selection model vs OT transforms; HTML fidelity vs security. |
| Production ABI | No change unless a later model/document ABI is signed separately. |

**Sign-off**

- [ ] Product: approve axis **A (HTML)** or **B (OT)** (circle one)
- [ ] Engineering: capacity and regression plan approved
- [ ] Implementation wave id / date: ________

---

## 4. GTK true menus

| Field | Content |
|-------|---------|
| Motivation | Linux hosts with a real GTK window want native menus; pure winit still falls back. |
| Suggested MVP (only after sign-off) | Wire `DesktopMenu::attach_gtk_hint` / `init_for_gtk_window` path on Linux GTK hosts; document fallback for pure winit. |
| Explicitly still out | Making muda the sole menu API; moving GTK menus into `synui::prelude`; replacing in-app `MenuBar`/`ContextMenu`. |
| Dependencies | Existing platform shell docs (`desktop_shell`); in-app menus remain default for facade demos. |
| Demo value | Platform shell samples only — not ordinary gallery evidence. |
| Risks | Dual menu stacks (in-app + OS); focus ownership on hybrid hosts. |
| Production ABI | No change. |

**Sign-off**

- [ ] Product: approve GTK true-menu MVP scope above
- [ ] Engineering: capacity and regression plan approved
- [ ] Implementation wave id / date: ________

---

## Global non-goals (remain locked until separate sign-off)

- Restoring the Engine gold archive or `legacy` feature
- Expanding Production ABI beyond `synui_v1` 1.1 for these unlocks
- Plugin CDN / cold `layout_10k` ≤8ms claims
- Sharing `LottiePlayer` / `velato::Renderer` across widgets

## How to use this packet

1. Product picks the next unlock using the recommended order (or overrides in writing).
2. Both checkboxes for that item must be checked before a Phase/Wave plan may include implementation todos.
3. Until then, gap §9 and DEVELOPMENT Non-goals keep the item hard-locked.
