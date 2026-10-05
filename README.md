# Apply Tool

A local-only desktop app (Tauri 2 + SvelteKit + SQLite) that turns a job posting into a tailored CV and a one-page motivation letter, and tracks your applications. No account, no server.

## Flow
1. **New application:** paste a link (LinkedIn, XING, StepStone, …; JSON-LD JobPosting data is extracted), the job text, and/or screenshots (pick files or Ctrl+V).
2. **Extract:** your AI CLI reads everything, including the screenshots, and writes `job.json`.
3. **Company check:** if the company is on your list, the job is flagged and the flow stops ("Generate anyway" is available). Otherwise it is marked appliable.
4. **Generate:** first the English CV and motivation letter (`cv/CV_en.typ`, `cv/Letter_en.typ`), shown as soon as they are rendered. Then, in the background, the German versions (`cv/CV_de.typ`, `cv/Letter_de.typ`) mirroring them. All follow `SKILLS.md`.
5. **Edit by hand:** "✎ Edit" opens any document in an editor with a live preview, where the layout code is hidden and the text uses Markdown-like Typst markup. Saving regenerates PDF/DOCX/ODT, and the AI updates the other language from your edits.
5. **Render:** the bundled `typst` produces the PDF and in-app previews, and the bundled `pandoc` produces DOCX and ODT. If the letter runs over one A4 page, the AI is asked to shorten it automatically.
6. **Refine and track:** ask the AI for changes in the job's chat. Set the status (draft → ready → applied → interview → offer / rejected / no response) and archive the job. The tracker offers a filterable, sortable table, a drag-and-drop board and CSV export.

## Data (`~/ApplyTool`)
| Path | What |
|---|---|
| `apply.db` | SQLite: jobs, company list, settings |
| `skills/*.md` | Rules the AI must follow, combined into `SKILLS.md` for every run (editable in Settings → Skills) |
| `cv/master.typ`, `cv/letter.typ` | Your master CV and letter template in Typst (Settings → Master CV, with "Import my CV" from DOCX/ODT/PDF) |
| `jobs/NNNN/` | Per application: `input/`, `job.json`, `cv/*.typ`, `output/*.{pdf,docx,odt}` |
| `applications/` | A frozen snapshot per application, made when you first mark it as applied: the documents you sent, the posting and `APPLICATION.md` with your notes at that time |
| `fonts/` | Extra fonts for Typst |

## AI agents
The app uses the CLI already installed on your system (`claude`, `codex` or `opencode`) with your own login or API key. From the Flatpak it runs them via `flatpak-spawn --host`. Choose the model in Settings → General: Claude Code aliases/IDs, `codex debug models`, or `opencode models opencode-go`. You can edit the commands in Settings → Agent commands.

## Dev
    nix develop -c pnpm install
    nix develop -c pnpm tauri dev

## Flatpak
    ./build-flatpak.sh
    flatpak run io.github.trifazor.ApplyTool

Typst and pandoc are bundled, so LibreOffice is not needed.

## Install and releases

See [installation instructions](docs/INSTALL.md) and [local release pipeline setup](docs/RELEASING.md). Every push to master builds on the registered local runner and publishes the signed Flatpak update repository and GitHub release.
