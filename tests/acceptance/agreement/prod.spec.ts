import { test, expect, type APIRequestContext } from '@playwright/test';
import { apex, hostHeaders } from '../fixtures/env.js';

// One run per domain, started through the API without streaming; every view of the stored
// entry must then say the same thing: grade, open issues, entry ID (SDD 9.1, production).

const DOMAINS = ['netray.info', 'example.com'];
const PROTOCOLS = ['dns', 'tls', 'http', 'email', 'ip'];

const origin = apex();
const headers = hostHeaders(origin);

interface ApiRun {
  grade: string;
  checks: { id: string; status: string }[];
}

async function get(request: APIRequestContext, path: string): Promise<string> {
  const response = await request.get(`${origin.url}${path}`, { headers });
  expect(response.status(), path).toBe(200);
  return response.text();
}

// TODO(v2-result-page): confirm selector. The grade letter is the `.grade__letter` of the
// grade component (crates/web/templates/components/grade.html).
function viewGrade(html: string): string {
  const match = html.match(/<span class="grade__letter">([^<]+)<\/span>/);
  expect(match, 'grade letter in the result view').not.toBeNull();
  return match![1].trim();
}

// TODO(v2-result-page): confirm selector, and scope it to the open-issues block once its
// marker exists. The check-row component also renders passed checks and the evidence copies of
// each issue (SDD §5, §5.8), so only warn and fail rows count, each once.
function viewIssues(html: string): string[] {
  const rows = [
    ...html.matchAll(
      /<span class="check-row__code">([^<]+)<\/span>\s*<span class="check-row__tone">([^<]+)<\/span>/g,
    ),
  ];
  const issues = rows
    .map((m) => [m[1].trim(), m[2].trim()])
    .filter(([, tone]) => tone === 'warn' || tone === 'fail')
    .map(([code, tone]) => `${code} ${tone}`);
  return [...new Set(issues)].sort();
}

// TODO(v2-result-page): confirm selector. The entry's own address `/r/<id>` is what main's
// entry view carries in `og:url`; the result and evidence views are expected to carry it too.
function viewEntryId(html: string): string {
  const match = html.match(/<meta property="og:url" content="[^"]*\/r\/([0-9A-Za-z]{8})"/);
  expect(match, 'entry ID in og:url').not.toBeNull();
  return match![1];
}

function badgeLetter(svg: string): string {
  const texts = [...svg.matchAll(/<text[^>]*>([^<]*)<\/text>/g)];
  expect(texts.length, 'text elements in the badge').toBeGreaterThan(0);
  return texts[texts.length - 1][1].trim();
}

for (const domain of DOMAINS) {
  test(`${domain}: page, API, badge and evidence views agree`, async ({ request }) => {
    test.setTimeout(180_000);

    const started = await request.post(`${origin.url}/api/v1/domains/${domain}/runs`, { headers });
    expect(started.status(), 'run answer').toBe(200);
    const answer = await started.json();
    expect(answer.entry_id, `${domain} run was recorded (${answer.recorded})`).toBeTruthy();
    const run: ApiRun = answer.run;

    const apiIssues = run.checks
      .filter((c) => c.status === 'warn' || c.status === 'fail')
      .map((c) => `${c.id} ${c.status}`)
      .sort();

    const result = await get(request, `/${domain}`);
    expect(viewGrade(result), 'page grade equals API grade').toBe(run.grade);
    expect(viewIssues(result), 'open issues equal the API warn and fail checks').toEqual(apiIssues);
    expect(viewEntryId(result), 'page names the API entry').toBe(answer.entry_id);

    const badge = await get(request, `/badge/${domain}.svg`);
    expect(badgeLetter(badge), 'badge letter equals API grade').toBe(run.grade);

    for (const protocol of PROTOCOLS) {
      const evidence = await get(request, `/${domain}/${protocol}`);
      expect(viewEntryId(evidence), `${protocol} evidence view names the same entry`).toBe(
        answer.entry_id,
      );
    }
  });
}
