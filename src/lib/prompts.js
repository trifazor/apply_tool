// All context is inlined by the backend ({{skills}}, {{file:…}}, {{screenshots}}) and the agent answers with
// <<<FILE name>>> … <<<END>>> blocks that the backend writes — so a run is a single model turn with no tool calls.

const OUTPUT_RULES = `OUTPUT FORMAT — IMPORTANT:
- Do NOT use any tools (no reading, writing, searching, shell, todo lists). Everything you need is in this message.
- Answer with the complete content of each requested file, exactly like this:
<<<FILE path/name.ext>>>
…full file content…
<<<END>>>
- No markdown code fences around file contents. After the blocks, add at most 3 short lines summarising what you did.`;

export const extractPrompt = () => `Extract the job posting below into structured JSON.

${OUTPUT_RULES}

Screenshots of the job ad: {{screenshots}}
(If screenshots are listed, read those image files — that is the ONLY tool use allowed. Otherwise use no tools.)

Requested file: job.json — valid JSON with exactly these keys:
{"title":"","company":"","location":"","language":"de|en|…","employment_type":"","contact_person":"","contact_email":"","salary":"","summary":"","responsibilities":[],"requirements":[],"nice_to_have":[],"keywords":[],"url":""}
Use "" or [] when unknown. "company" is the hiring company's plain name (not the job board or recruiter platform).

=== POSTING (input/job.txt) ===
{{file:input/job.txt}}`;

const context = () => `=== RULES (SKILLS.md) — follow all of them ===
{{skills}}

=== JOB (job.json) ===
{{file:job.json}}

=== MASTER CV (cv/master.typ) ===
{{file:cv/master.typ}}

=== LETTER TEMPLATE (cv/letter.typ) ===
{{file:cv/letter.typ}}`;

const today = () => new Date().toISOString().slice(0, 10);

/** Phase 1: the English CV and letter, shown as soon as they are rendered. */
export const englishPrompt = () => `You are tailoring a job application. Today is ${today()}.

${OUTPUT_RULES}

Requested files (both in ENGLISH):
1. cv/CV_en.typ — the master CV tailored to this job. Keep the master's styling/setup code; adapt the content. Use \`#set text(lang: "en")\`.
2. cv/Letter_en.typ — the motivation letter for this job, based on the letter template, with the applicant's details from the CV. It MUST fit on one A4 page. Use \`#set text(lang: "en")\`.

${context()}`;

/** Phase 2 (background): German versions mirroring the English documents. */
export const germanPrompt = () => `The tailored ENGLISH CV and motivation letter are finished (below). Create the GERMAN versions.

${OUTPUT_RULES}

Requested files:
1. cv/CV_de.typ — German version of cv/CV_en.typ: same selection, order and emphasis, same styling/setup code, natural German (not word for word), German section titles and date formats. Use \`#set text(lang: "de")\`.
2. cv/Letter_de.typ — German version of cv/Letter_en.typ: same structure and arguments, formal "Sie", German date line and closing. It MUST fit on one A4 page. Use \`#set text(lang: "de")\`.

=== RULES (SKILLS.md) ===
{{skills}}

=== JOB (job.json) ===
{{file:job.json}}

=== ENGLISH CV (cv/CV_en.typ) ===
{{file:cv/CV_en.typ}}

=== ENGLISH LETTER (cv/Letter_en.typ) ===
{{file:cv/Letter_en.typ}}`;

const LANG = { en: 'English', de: 'German' };

/** After a manual edit: bring the other language in line with the edited document. */
export const syncTranslationPrompt = (kind, from, to) => `The applicant manually edited the ${LANG[from]} ${kind === 'CV' ? 'CV' : 'motivation letter'} (cv/${kind}_${from}.typ). Their edited version is now the source of truth.
Update cv/${kind}_${to}.typ so it is a faithful ${LANG[to]} version of the edited document: carry over every content change (added, removed, reworded or reordered parts), keep cv/${kind}_${to}.typ's styling/setup code, write natural ${LANG[to]}.${kind === 'Letter' ? ' It MUST fit on one A4 page.' : ''}

${OUTPUT_RULES}

Requested file: cv/${kind}_${to}.typ

=== RULES (SKILLS.md) ===
{{skills}}

=== EDITED ${LANG[from].toUpperCase()} VERSION (cv/${kind}_${from}.typ) ===
{{file:cv/${kind}_${from}.typ}}

=== CURRENT ${LANG[to].toUpperCase()} VERSION (cv/${kind}_${to}.typ) ===
{{file:cv/${kind}_${to}.typ}}`;

const current = () => ['CV_en', 'Letter_en', 'CV_de', 'Letter_de']
  .map((f) => `=== CURRENT cv/${f}.typ ===\n{{file:cv/${f}.typ}}`).join('\n\n');

export const shortenPrompt = (file, pages) => `The motivation letter cv/${file} renders to ${pages} A4 pages. Rewrite it so it fits on ONE page: tighten the wording and keep the strongest points. Keep the layout code unchanged.

${OUTPUT_RULES}

Requested file: cv/${file}

=== RULES (SKILLS.md) ===
{{skills}}

=== CURRENT cv/${file} ===
{{file:cv/${file}}}`;

export const fixPrompt = (file, error) => `The Typst file cv/${file} fails to compile. Fix the error with minimal changes and keep the content.

${OUTPUT_RULES}

Requested file: cv/${file}

=== TYPST ERROR ===
${error}

=== CURRENT cv/${file} ===
{{file:cv/${file}}}`;

export const followUpPrompt = (msg) => `The applicant asks for this change to their application documents:
"${msg}"

${OUTPUT_RULES}

Return ONLY the files you changed (any of cv/CV_en.typ, cv/Letter_en.typ, cv/CV_de.typ, cv/Letter_de.typ). Keep the English and German versions in sync.

${context()}

${current()}`;

export const importCvPrompt = (file) => `Convert the applicant's existing CV into a Typst master CV.

${OUTPUT_RULES}
${file.endsWith('.pdf') ? `(Exception: read ./${file} — the CV is a PDF — that is the only tool use allowed.)` : ''}

Requested files:
1. master.typ — contains ALL information of the source CV (every position, date, bullet, education, skill, language, contact detail); do not summarise or omit anything. Recreate the source's visual style as closely as the Typst rules allow, using the current master.typ as a structural example.
2. letter.typ — the current letter template with the sender block updated to the applicant's name and contact details (keep the rest).

=== RULES (SKILLS.md) ===
{{skills}}

=== SOURCE CV (text version) ===
${file.endsWith('.pdf') ? '(see the PDF)' : '{{file:source.md}}'}

=== CURRENT master.typ ===
{{file:master.typ}}

=== CURRENT letter.typ ===
{{file:letter.typ}}`;
