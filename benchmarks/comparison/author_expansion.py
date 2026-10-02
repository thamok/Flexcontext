"""Freeze source-reviewed, module-disjoint questions against the immutable pilot snapshots."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAPSHOTS = ROOT / '.benchmark-results/pilot-20260916/snapshots'
cases = []

def add(repo, split, ident, question, path, start, end, atoms):
    lines = (SNAPSHOTS / repo / path).read_text().splitlines(keepends=True)
    # Region boundaries are authored source anchors, never retrieval results.
    a = next(i for i, line in enumerate(lines) if start in line)
    b = next((i for i in range(a + 1, len(lines)) if end and end in lines[i]), len(lines))
    def span(i, j):
        return dict(path=path, start=i + 1, end=j, sha256=hashlib.sha256(''.join(lines[i:j]).encode()).hexdigest())
    evidence = []
    for number, fragments in enumerate(atoms):
        spans = []
        for fragment in fragments:
            matches = [i for i in range(a,b) if fragment in lines[i]]
            assert len(matches) == 1, (ident, fragment, matches)
            spans.append(span(matches[0],matches[0]+1))
        evidence.append(dict(id=f'e{number+1}',spans=spans))
    cases.append(dict(id=ident,repo=repo,split=split,module=path,question=question,evidence=evidence,relevant_regions=[span(a,b)]))

p='src/lexical.rs'
add('flexcontext','dev','v2-identifier-boundaries','How are camel case, acronym transitions, and alphabetic to numeric identifier boundaries split?',p,'pub fn identifier_tokens','fn push_word',[["prev.is_lowercase() && ch.is_uppercase()"],["prev.is_alphabetic() != ch.is_alphabetic()"],["previous.is_some_and(char::is_uppercase)","next.is_some_and(char::is_lowercase)"]])
add('flexcontext','dev','v2-identifier-punctuation','How does identifier tokenization handle punctuation and normalize letter case?',p,'pub fn identifier_tokens','fn push_word',[["if !ch.is_alphanumeric() {"],["current.extend(ch.to_lowercase());"]])
add('flexcontext','dev','v2-word-related','When are lexical terms treated as related through stems or prefixes, and what minimum length is required?',p,'pub fn lexically_related','pub fn light_stem',[["if left == right {"],["left_stem.len() >= 4 && left_stem == right_stem"],["left.len() >= 4 && right.starts_with(left)","right.len() >= 4 && left.starts_with(right)"]])
p='src/index.rs'
add('flexcontext','dev','v2-posting-keys','Which exact, stem, and prefix keys are stored for each lexical token?',p,'pub(crate) fn token_keys','#[cfg(test)]',[["format!(\"t:{token}\"), format!(\"s:{}\", light_stem(token))"],["token.chars().take(4).collect()","keys.insert(format!(\"p:{prefix}\"));"]])
add('flexcontext','dev','v2-candidate-union','How does the lexical index combine normalized symbol names and token postings into deduplicated candidates?',p,'pub fn candidates','pub fn posting_count',[["let mut candidates = BTreeSet::new();"],["self.postings.get(&format!(\"n:{}\", query.normalized))"],["for key in token_keys(token)","candidates.into_iter().collect()"]])
add('flexcontext','dev','v2-index-fields','Which symbol fields contribute lexical posting keys during index construction?',p,'pub fn build_iter','pub fn candidates',[["&symbol.path,","&symbol.comments(),"],["symbol.signature(),","symbol.body(),"],["for identifier in &symbol.identifiers {"]])
p='src/parser.rs'
add('flexcontext','heldout','v2-parser-reuse','How does the symbol extractor reuse parsers across languages and detect parser cancellation?',p,'pub fn extract(&mut self','impl Default',[["self.parsers.entry(file.language)","entry.insert(parser);"],["Tree-sitter cancelled parsing"]])
add('flexcontext','heldout','v2-parser-facts','How are imports and identifier, type, and call facts deduplicated and assigned to extracted symbols?',p,'    imports.sort();','fn symbol_name',[["imports.dedup();","let imports: Arc<[String]> = imports.into();"],["symbol.identifiers = identifiers.into_iter()","symbol.type_references = types.into_iter()","symbol.calls = calls.into_iter()"]])
add('flexcontext','heldout','v2-parser-comments','When are preceding comments attached to a source symbol, and what breaks the comment chain?',p,'fn comment_start_byte','fn is_comment_kind',[["next_start_line.saturating_sub(previous.end_position().row) > 2"],["gap.lines().any(|line| !line.trim().is_empty())"]])
p='src/mcp.rs'
add('flexcontext','heldout','v2-mcp-cancellation','How does the MCP reader handle cancellation notifications and signal pending requests?',p,'pub fn serve(','fn process_requests',[["value[\"method\"] == \"notifications/cancelled\"","value.get(\"id\").is_none()"],["let key = value[\"params\"][\"requestId\"].to_string();","flag.store(true, Ordering::Relaxed);"]])
add('flexcontext','heldout','v2-mcp-modern-meta','Which per-request protocol metadata does modern MCP require and how are unsupported versions rejected?',p,'        let meta =','        let result =',[["meta[\"io.modelcontextprotocol/protocolVersion\"].as_str()"],["if !meta[\"io.modelcontextprotocol/clientCapabilities\"].is_object()"],["if version != PROTOCOL_VERSION {","\"code\":-32022"]])
add('flexcontext','heldout','v2-mcp-legacy-wait','How does MCP reject requests while a legacy session is waiting for the initialized notification?',p,'        let meta =','        let result =',[["} else if !modern && legacy_ready == Some(false) {"],["error(id, -32600, \"Awaiting notifications/initialized\")"]])
p='apps/chat-agentx/packages/api/src/utils/promise.ts'
add('agentx','dev','v2-promise-timeout','How does a promise timeout log and reject an operation and clear its timer after completion?',p,'export async function withTimeout','/**',[["if (logger) logger(error.message, error);","reject(error);"],["return await Promise.race([promise, timeoutPromise]);","clearTimeout(timeoutId!);"]])
add('agentx','dev','v2-limiter-queue','How does the concurrency limiter validate capacity and queue work when all slots are occupied?',p,'export function createConcurrencyLimiter',None,[["if (!Number.isInteger(concurrency) || concurrency < 1) {"],["if (active < concurrency) {","queue.push(run);"]])
add('agentx','dev','v2-limiter-release','How does the concurrency limiter release slots in FIFO order after successful or failed tasks?',p,'export function createConcurrencyLimiter',None,[["active--;","const next = queue.shift();"],["resolve(value);","reject(error);"]])
p='apps/chat-agentx/packages/api/src/utils/url.ts'
add('agentx','dev','v2-base-url-special','How does API base URL extraction handle invalid input, Cohere endpoints, and URLs without a version segment?',p,'export function extractBaseURL','/**',[["if (!url || typeof url !== 'string') {","return undefined;"],["if (url.startsWith(CohereConstants.API_URL)) {","return null;"],["if (!url.includes('/v1')) {","return url;"]])
add('agentx','dev','v2-azure-url','How does base URL extraction strip chat or completion paths from an Azure OpenAI proxy URL?',p,'export function extractBaseURL','/**',[["if (suffixUsed === 'azure-openai') {","return url.split(/\\/(chat|completion)/)[0];"]])
add('agentx','dev','v2-url-origin','How is an endpoint origin reconstructed from protocol, hostname and port, and what happens when parsing fails?',p,'export function deriveBaseURL',None,[["const parsedUrl = new URL(fullURL);","const port = parsedUrl.port;"],["return `${protocol}//${hostname}${port ? `:${port}` : ''}`;"],["logger.error('Failed to derive base URL', error);"]])
p='apps/chat-agentx/packages/api/src/utils/sanitizeTitle.ts'
add('agentx','heldout','v2-title-clean','How are reasoning blocks removed and whitespace normalized in generated conversation titles?',p,'export function sanitizeTitle',None,[["const thinkBlockRegex =","const cleaned = rawTitle.replace(thinkBlockRegex, '');"],["const normalized = cleaned.replace(/\\s+/g, ' ');","const trimmed = normalized.trim();"]])
add('agentx','heldout','v2-title-unicode','How are long conversation titles truncated without splitting Unicode code points?',p,'export function sanitizeTitle',None,[["const codePoints = [...trimmed];","const truncateAt = MAX_TITLE_LENGTH - 3;"],["return codePoints.slice(0, truncateAt).join('').trimEnd() + '...';"]])
add('agentx','heldout','v2-title-empty','Which invalid or empty generated title inputs trigger the default fallback?',p,'export function sanitizeTitle',None,[["if (!rawTitle || typeof rawTitle !== 'string') {"],["if (trimmed.length === 0) {"]])
p='apps/chat-agentx/packages/api/src/utils/memory.ts'
add('agentx','heldout','v2-memory-history','How is memory snapshot history bounded and when is a trend calculation skipped?',p,'function collectSnapshot','function forceGC',[["if (snapshots.length > SNAPSHOT_HISTORY_LIMIT) {","snapshots.shift();"],["if (snapshots.length < 3) {"],["if (elapsedMin < 0.1) {"]])
add('agentx','heldout','v2-memory-gc','When can memory diagnostics force garbage collection and how does it report unsupported runtimes?',p,'function forceGC','function getSnapshots',[["if (global.gc) {","global.gc();","return true;"],["return false;"]])
add('agentx','heldout','v2-memory-start','How does memory diagnostics avoid duplicate timers and allow the process to exit naturally?',p,'function start','function stop',[["if (interval) {"],["collectSnapshot();","interval = setInterval(collectSnapshot, INTERVAL_MS);"],["interval.unref();"]])
p='Tools/Scripts/webkitpy/common/system/filesystem.py'
add('webkitpy','dev','v2-files-single','How does filesystem enumeration handle a root that is already a file and apply the file filter?',p,'    def files_under','    def getcwd',[["if self.isfile(path):","if file_filter(self, self.dirname(path), self.basename(path)):","files.append(path)"]])
add('webkitpy','dev','v2-files-prune','How does recursive filesystem enumeration prune skipped directory names including the root?',p,'    def files_under','    def getcwd',[["if self.basename(path) in dirs_to_skip:","return []"],["if d in dirnames:","dirnames.remove(d)"]])
add('webkitpy','dev','v2-dirs-filter','How does directory enumeration apply an optional callback during a top-down walk?',p,'    def dirs_under','    def files_under',[["dir_filter = dir_filter or filter_all"],["it = os.walk(path)","if dir_filter(self, dirpath):","dirs.append(dirpath)"]])
p='Tools/Scripts/webkitpy/common/memoized.py'
add('webkitpy','dev','v2-memo-init','How does the memoization decorator initialize its wrapped callable and result cache?',p,'    def __init__','    def __call__',[["self._function = function","self._results_cache = {}"]])
add('webkitpy','dev','v2-memo-call','How does memoization distinguish a cached call from a cache miss and store the new result?',p,'    def __call__','    def __get__',[["return self._results_cache[args]"],["except KeyError:","result = self._function(*args)","self._results_cache[args] = result"]])
add('webkitpy','dev','v2-memo-method','How does the memoization decorator bind an instance when it is used as a method descriptor?',p,'    def __get__',None,[["return functools.partial(self.__call__, instance)"]])
p='Tools/Scripts/webkitpy/common/config/urls.py'
add('webkitpy','heldout','v2-revision-url','How are numeric revisions distinguished from commit identifiers when constructing a WebKit revision URL?',p,'def view_revision_url','def view_identifier_url',[["if isinstance(revision_number, int) or revision_number.isdigit():","return 'https://commits.webkit.org/r{}'.format(revision_number)"],["return 'https://commits.webkit.org/{}'.format(revision_number)"]])
add('webkitpy','heldout','v2-bug-url','How are short and long bug URLs parsed into numeric bug identifiers?',p,'def parse_bug_id','def parse_attachment_id',[["match = re.search(bug_url_short, string)"],["match = re.search(bug_url_long, string)"]])
add('webkitpy','heldout','v2-attachment-url','Which URL patterns are used to extract attachment identifiers?',p,'def parse_attachment_id',None,[["match = re.search(attachment_url, string)"],["match = re.search(direct_attachment_url, string)"]])
p='Tools/Scripts/webkitpy/common/system/executive.py'
add('webkitpy','heldout','v2-error-tail','How are long command error outputs shortened and what is retained in the error message?',p,'    def message_with_output','    def command_name',[["if output_limit and len(self.output) > output_limit:","(self, output_limit, self.output[-output_limit:])"],["return unicode(self)"]])
add('webkitpy','heldout','v2-cpu-count','When does process execution use the configured processor count and when does it fall back to the system count?',p,'    def cpu_count','    def kill_process',[["cpus = int(os.environ.get('NUMBER_OF_PROCESSORS'))","if cpus > 0:","return cpus"],["return multiprocessing.cpu_count()"]])
add('webkitpy','heldout','v2-kill-invalid','How does process termination guard against invalid process IDs and handle Windows process trees?',p,'    def kill_process','    def _win32_check_running_pid',[["if pid is None or pid <= 0:","raise RuntimeError('Cannot kill process with invalid pid"],["command = [task_kill_executable, \"/f\", \"/t\", \"/pid\", pid]","self.run_command(command, ignore_errors=True)"]])

if __name__ == '__main__':
    assert len(cases) == 36
    for repo in ('flexcontext','agentx','webkitpy'):
        for split in ('dev','heldout'):
            assert len([c for c in cases if c['repo']==repo and c['split']==split]) == 6
    path = ROOT / 'benchmarks/comparison/optimization-cases.json'
    if path.exists():
        raise SystemExit('Corpus already frozen; do not silently overwrite judgments.')
    path.write_text(json.dumps({'schema_version':1,'source_snapshot':'pilot-20260916','judgment_status':'Source-reviewed and frozen before variant experiments; assistant authored, not independently adjudicated.','cases':cases},indent=2)+'\n')
    print(hashlib.sha256(path.read_bytes()).hexdigest())
