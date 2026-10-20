import { platformDisplayName } from "@/lib/platform-display";
import { DiscoveryPlanRequirements } from "./discovery-plan-requirements";
import { PINTEREST_MIN_MONTHLY_VIEWS_EXCLUSIVE } from "@onecrew/runtime-contracts/discovery";
import { discoveryProviderFailureKey } from "@/lib/discovery-execution-error";
import { isU64 } from "@onecrew/runtime-contracts/result-wire";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Download, ExternalLink, Loader2, Settings2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { canStartPreparedDraft, discoveryDraftFailureKey, matchesDiscoveryCodexRequest, type DiscoveryCodexRequest } from "@/lib/discovery-codex-request";
import { discoveryExecutionKey, getDiscoveryExecution, startDiscoveryExecution, pauseDiscoveryExecution, executionRunning } from "@/lib/discovery-execution";
import { discoveryDraftProgressKey, getDiscoveryDraftProgress } from "@/lib/discovery-draft-progress";
import { discoveryLibraryFailureKey } from "@/lib/discovery-library-error";
import { checkpointExecution, loadRunCheckpoint, loadRunHistory, type DiscoveryRunCheckpoint } from "@/lib/discovery-run-checkpoint";
import { authorGroupsKey, authorGroupsRefreshInterval, getDiscoveryAuthorGroups } from "@/lib/discovery-author-groups";
import { DISCOVERY_DEFAULTS, loadDiscoveryPreferences, saveDiscoveryPreferences, discoveryPlanningBrief, discoveryPlanningScopeOverrides, discoveryEditedScopes } from "@/lib/discovery-draft-preferences";
import { loadOpenCliAuthObservations, OPENCLI_AUTH_CHANGED, excludedSearchPlatforms, preflightOpenCliSearch } from "@/lib/opencli-auth";
import {
  approveDiscoveryCampaign, canRecoverDiscoverySearch, canVerifyDiscovery, countDiscoveryCandidates, createDiscoveryCampaign, getDiscoveryCampaign, getDiscoveryContent, prepareDiscoveryDraft,
  CATEGORY_SEARCH_PLATFORMS, TAG_SEARCH_PLATFORMS, discoveryScopeKeywords, listDiscoveryCandidates, listDiscoveryHits, nextDiscoveryPage, syncDiscoveryLibrary, validDiscoveryScope, verifyDiscoveryYoutubeHitOnce,
  searchDiscoveryOnce, type DiscoveryContentRead, type DiscoveryCampaign,
  listRecentDiscoveryCampaigns, validDiscoveryCampaignId, getDiscoveryLibrary,
} from "@/lib/opencli-discovery";
import { formatDate } from "@/lib/creator";
import type { OpenCliSetupReport } from "@/lib/opencli-setup";
import "@/styles/discovery.css";
import { discoveryStage, discoveryRunStageKey } from "@/lib/discovery-workspace";
import { DiscoveryOverview } from "./discovery-overview";
import { DiscoveryWorkspacePresence } from "./discovery-workspace";
import { DiscoveryResultsTable } from "./discovery-results-table";
import { DiscoveryTargetIncrease } from "./discovery-target-increase";
import { DiscoveryAuthorGroups } from "./discovery-author-groups";
import { SavedIdentityEvidence } from "./discovery-identity";
import { discoveryHitsCsv, loadAllDiscoveryHits } from "./library-csv-export";

// These are source-reviewed, read-only OpenCLI 1.8.8 entry points. A listed
// platform is not a promise that the selected Chrome identity is logged in.
// New drafts start with every registered entry. Saved Campaign scopes remain frozen.
export const SEARCH_PLATFORM_GROUPS = [
  { label: "overseas", platforms: ["youtube", "tiktok", "instagram", "twitter", "linkedin", "linkedin-salesnav", "facebook", "pinterest", "pinterest-pins", "pinterest-boards", "reddit", "reddit-subreddit", "producthunt-posts"] },
  { label: "chinese", platforms: ["bilibili", "weixin", "douyin", "weibo", "hupu", "juejin-hot", "weread", "douban-books", "douban-music"] },
  { label: "publications", platforms: ["medium", "medium-tag", "substack", "apple-podcasts", "archive", "devto-tag", "lesswrong-tag", "lobsters-tag", "linkedin-learning"] },
  { label: "other", platforms: ["bluesky", "jike", "hackernews", "linux-do", "nowcoder", "v2ex-node", "stackoverflow", "stackoverflow-tag", "stackoverflow-users", "hf-spaces", "hf-models", "hf-datasets", "dribbble-shots", "dribbble-designers", "google", "google-images", "duckduckgo", "yahoo"] },
] as const;

const SEARCH_PLATFORM_ORDER = SEARCH_PLATFORM_GROUPS.flatMap(group => group.platforms);

export function registeredSearchPlatforms(capabilities: OpenCliSetupReport["search_capabilities"]): string[] {
  return SEARCH_PLATFORM_ORDER.filter(platform =>
    capabilities.some(capability => capability.platform === platform && capability.registered));
}

const SAVED_CAMPAIGN_KEY = "onecrew-opencli-pilot-campaign";
function savedCampaignId(): string | null {
  if (typeof window === "undefined") return null;
  try { return window.localStorage.getItem(SAVED_CAMPAIGN_KEY); }
  catch { return null; }
}

export function SavedContentEvidence({ read }: { read: DiscoveryContentRead }) {
  const { t, i18n } = useTranslation("shell");
  const content = read.native_content;
  return <div className="mt-2 space-y-1 text-xs">
    <p>{read.status}{read.stop_code ? ` · ${t(discoveryProviderFailureKey(read.stop_code))}` : ""}</p>
    {content && "bvid" in content && <>
      <p>{content.title} · MID {read.claim.author_id}</p>
      <p className="whitespace-pre-wrap break-words">{content.description}</p>
      {content.description_truncated && <p>{i18n.resolvedLanguage?.startsWith("zh") ? "简介已截断" : "Description truncated"}</p>}
      {content.requires_payment && <p>{i18n.resolvedLanguage?.startsWith("zh") ? "完整视频需付费，演示内容未核实。" : "Full video requires payment; demonstration remains unverified."}</p>}
      {content.payment_status_known === false && <p>{i18n.resolvedLanguage?.startsWith("zh") ? "公开视频页未提供付费状态；不能据此断定免费可观看。" : "The public page did not verify payment status; free playback is not established."}</p>}
      <p className="text-muted-foreground">{i18n.resolvedLanguage?.startsWith("zh") ? "公开页面文字仅供复核，尚不能证明视频中有真实演示。" : "Public page text needs review; it does not prove an on-video demonstration."}</p>
    </>}
    {content && "original_posts" in content && <>
      <p>{t("opencliPilot.xRecentPosts", { sampled: content.sampled_posts,
        original: content.original_posts.length, retweets: content.excluded_retweets })}</p>
      {content.original_posts.length === 0 && <p>{t("opencliPilot.xNoOriginalPosts")}</p>}
      <ul className="space-y-2">{content.original_posts.map(post =>
        <li key={post.id}><a href={post.source_url} target="_blank" rel="noreferrer" className="text-primary underline">{post.created_at || post.id}</a>
          <p className="whitespace-pre-wrap break-words">{post.text}</p></li>)}</ul>
      <p className="text-muted-foreground">{t("opencliPilot.xContentLimit")}</p>
    </>}
    {content && "text" in content && !("identity" in content) && <>
      <p>{t("opencliPilot.reviewNativeContent")}</p>
      <a href={content.source_url} target="_blank" rel="noreferrer" className="text-primary underline">{content.source_url}</a>
      <p className="whitespace-pre-wrap break-words">{content.text}</p>
      {"text_truncated" in content && content.text_truncated && <p>{t("opencliPilot.reviewTruncated")}</p>}
    </>}
    {content && "identity" in content && <>
      <p>{t("opencliPilot.xNativeContent", { account: content.identity.account_id })}</p>
      <a href={content.identity.source_url} target="_blank" rel="noreferrer" className="text-primary underline">{content.identity.source_url}</a>
      <p className="whitespace-pre-wrap break-words">{content.text}</p>
      {content.text_truncated && <p>{t("opencliPilot.xContentTruncated")}</p>}
      <ul>{content.expanded_urls.map(url => <li key={url} className="break-all">{url}</li>)}</ul>
      <p className="text-muted-foreground">{t("opencliPilot.xNativeContentLimit")}</p>
    </>}
  </div>;
}

/** Pilot search only: every result remains an unverified lead, never a Creator. */
export function DiscoveryPilot({ profileRef, version, capabilities, credentialId = null }: {
  credentialId?: string | null;
  profileRef: string;
  version: string;
  capabilities: OpenCliSetupReport["search_capabilities"];
}) {
  const { t, i18n } = useTranslation("shell");
  const queryClient = useQueryClient();
  const [savedPreferences] = useState(() => { try { return loadDiscoveryPreferences(window.localStorage); } catch { return null; } });
  const [preferencesStatus, setPreferencesStatus] = useState<"saved" | "failed" | "needsProduct" | null>(null);
  const [pendingAutoApproval, setPendingAutoApproval] = useState<{ id: string; profileRef: string } | null>(null);
  const consumedAutoApprovals = useRef(new Set<string>());
  const [campaignId, setCampaignId] = useState(savedCampaignId);
  const [restoreId, setRestoreId] = useState("");
  const [restoreOpen, setRestoreOpen] = useState(false);
  const recentCampaigns = useQuery({
    queryKey: ["opencli-recent-campaigns"], queryFn: listRecentDiscoveryCampaigns,
    enabled: restoreOpen, retry: false,
    // MCP can create drafts while this local-read picker is open.
    staleTime: 0, refetchOnWindowFocus: true, refetchInterval: restoreOpen ? 5_000 : false,
  });
  const [draftId, setDraftId] = useState(() => crypto.randomUUID());
  const [brief, setBrief] = useState(savedPreferences?.brief ?? "");
  const [expandedFrom, setExpandedFrom] = useState<string | null>(null);
  const [editingFrom, setEditingFrom] = useState<string | null>(null);
  const [authRevision, setAuthRevision] = useState(0);
  useEffect(() => {
    const changed = () => setAuthRevision(value => value + 1);
    window.addEventListener(OPENCLI_AUTH_CHANGED, changed);
    window.addEventListener("storage", changed);
    return () => { window.removeEventListener(OPENCLI_AUTH_CHANGED, changed); window.removeEventListener("storage", changed); };
  }, []);
  const excludedPlatforms = useMemo(() => excludedSearchPlatforms(loadOpenCliAuthObservations(credentialId, profileRef), profileRef, capabilities), [credentialId, profileRef, capabilities, authRevision]);
  const aiDraft = useMutation({
    retry: false,
    mutationFn: async (request: DiscoveryCodexRequest) => {
      const result = await prepareDiscoveryDraft({ id: request.id, brief: request.brief,
        profile_ref: request.profileRef, opencli_version: request.version,
        target_count: request.targetCount, max_calls: request.maxCalls, read_budget: request.readBudget, min_followers_override: request.minFollowersOverride, criteria_overrides: request.criteriaOverrides, scope_overrides: request.scopeOverrides, refinement: request.refinement });
      if (!matchesDiscoveryCodexRequest(result, request)) throw new Error("discovery_draft_conflict");
      return result;
    },
    onSuccess: (result, request) => {
      if (request.profileRef !== profileRef) return;
      queryClient.setQueryData(["opencli-campaign", result.id], result);
      setCampaignId(result.id);
      try { window.localStorage.setItem(SAVED_CAMPAIGN_KEY, result.id); } catch { /* Current page remains usable. */ }
      if (request.startAfterPrepare) setPendingAutoApproval({ id: result.id, profileRef: request.profileRef });
    },
    onError: () => setPreferencesStatus(null),
    onSettled: (_result, _error, request) => queryClient.invalidateQueries({ queryKey: discoveryDraftProgressKey(request.id) }),
  });
  // No runtime event covers process-local planning phases. Poll only this
  // admitted request while it is pending, then read its final snapshot once.
  const draftProgress = useQuery({
    queryKey: discoveryDraftProgressKey(aiDraft.variables?.id ?? draftId),
    queryFn: () => getDiscoveryDraftProgress(aiDraft.variables?.id ?? draftId),
    enabled: !!aiDraft.variables, retry: false, refetchInterval: aiDraft.isPending ? 1000 : false,
  });
  const [keywords, setKeywords] = useState(savedPreferences?.keywords ?? "");
  const [planningSeconds, setPlanningSeconds] = useState(0);
  useEffect(() => {
    setPlanningSeconds(0);
    if (!aiDraft.isPending) return;
    const started = Date.now();
    const timer = window.setInterval(() => setPlanningSeconds(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [aiDraft.isPending]);
  const [v2exNodes, setV2exNodes] = useState(savedPreferences?.v2exNodes ?? "");
  const [redditCommunities, setRedditCommunities] = useState(savedPreferences?.redditCommunities ?? "");
  const [tagInputs, setTagInputs] = useState<Record<string, string>>(savedPreferences?.tagInputs ?? {});
  const [categoryInputs, setCategoryInputs] = useState<Record<string, string>>(savedPreferences?.categoryInputs ?? {});
  const [selectedPlatforms, setPlatforms] = useState<string[]>(() => savedPreferences?.platforms ?? registeredSearchPlatforms(capabilities).filter(platform => platform === "youtube"));
  const [platformsEdited, setPlatformsEdited] = useState(false);
  const availablePlatforms = useMemo(() => registeredSearchPlatforms(capabilities), [capabilities]);
  const platforms = useMemo(() => selectedPlatforms.filter(platform => availablePlatforms.includes(platform) && !excludedPlatforms.includes(platform)), [selectedPlatforms, availablePlatforms, excludedPlatforms]);
  const selectedBrowserPlatforms = platforms.filter(platform =>
    capabilities.some(capability => capability.platform === platform && capability.browser_required));
  const browserPlatformSummary = selectedBrowserPlatforms.length > 6
    ? `${selectedBrowserPlatforms.slice(0, 6).map(platformDisplayName).join(", ")}… (+${selectedBrowserPlatforms.length - 6})`
    : selectedBrowserPlatforms.map(platformDisplayName).join(", ");
  const [activePlatform, setActivePlatform] = useState("youtube");
  const region = ""; // Retain the released preference shape; new plans do not filter residence.
  const [category, setCategory] = useState(savedPreferences?.category ?? "");
  const [language, setLanguage] = useState(savedPreferences?.language ?? "");
  const [targetInput, setTargetCount] = useState<number | "">(savedPreferences?.targetCount ?? DISCOVERY_DEFAULTS.targetCount);
  const [targetEdited, setTargetEdited] = useState(false);
  const targetCount = targetInput === "" ? NaN : targetInput;
  const [maxCalls, setMaxCalls] = useState<number | null>(DISCOVERY_DEFAULTS.maxCalls);
  const [budgetEdited, setBudgetEdited] = useState(false);
  // A new client's sentence must not inherit the old AI demo example as
  // unrequested hard criteria. Explicit values are reviewed before approval.
  const [followersInput, setMinSubscribers] = useState<number | bigint | "">(savedPreferences?.followersEdited ? savedPreferences.minSubscribers : DISCOVERY_DEFAULTS.minSubscribers);
  const minSubscribers = followersInput === "" ? NaN : followersInput;
  const [followersEdited, setFollowersEdited] = useState(savedPreferences?.followersEdited ?? false);
  const [requireDemo, setRequireDemo] = useState(false);
  const [requireToolLink, setRequireToolLink] = useState(false);
  useEffect(() => { setPreferencesStatus(null); }, [brief, region, language, category, minSubscribers, targetCount, maxCalls, keywords, platforms, v2exNodes, redditCommunities, tagInputs, categoryInputs, requireDemo, requireToolLink]);
  const editingSource = editingFrom ? queryClient.getQueryData<DiscoveryCampaign>(["opencli-campaign", editingFrom]) : undefined;
  const planningBrief = discoveryPlanningBrief({ brief, region,
    language: editingSource && language === (editingSource.criteria.language ?? "") ? "" : language,
    category: editingSource && category === (editingSource.criteria.category ?? "") ? "" : category, keywords,
    expandAllSources: expandedFrom !== null,
    defaultPlatforms: ["youtube"],
    platformRestriction: platformsEdited ? platforms : undefined });
  const planningBriefTooLong = new TextEncoder().encode(planningBrief).length > 4000;
  const execution = useQuery({ queryKey: discoveryExecutionKey(credentialId), queryFn: getDiscoveryExecution,
    enabled: !!credentialId, retry: false, refetchInterval: 1_500, refetchOnWindowFocus: true });
  const activeExecution = execution.data?.campaign_id === campaignId ? execution.data : null;
  const startExecution = useMutation({ mutationFn: startDiscoveryExecution, retry: false,
    onSuccess: result => queryClient.setQueryData(discoveryExecutionKey(credentialId), result),
    onSettled: () => queryClient.invalidateQueries({ queryKey: discoveryExecutionKey(credentialId) }) });
  const pauseExecution = useMutation({ mutationFn: pauseDiscoveryExecution, retry: false,
    onSuccess: result => queryClient.setQueryData(discoveryExecutionKey(credentialId), result),
    onSettled: () => queryClient.invalidateQueries({ queryKey: discoveryExecutionKey(credentialId) }) });
  const running = executionRunning(activeExecution) || startExecution.isPending;
  const pausing = activeExecution?.stage === "pausing" || pauseExecution.isPending;
  const [platformsOpen, setPlatformsOpen] = useState(false);
  const platformDetails = useRef<HTMLDivElement>(null);
  const [lastTables, setLastTables] = useState<Record<string, Awaited<ReturnType<typeof syncDiscoveryLibrary>>>>({});
  const [runCheckpoint, setRunCheckpoint] = useState(() => loadRunCheckpoint(window.localStorage, campaignId));
  const checkpointFailed = false;
  const executionStatus = activeExecution?.stage ?? checkpointExecution(runCheckpoint?.reason);
  const latestCheckpoint = useRef<DiscoveryRunCheckpoint | null>(runCheckpoint);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState(false);
  const reviewSyncPending = useRef(new Set<string>());
  const [syncFailure, setSyncFailure] = useState<{ campaignId: string; key: ReturnType<typeof discoveryLibraryFailureKey> } | null>(null);
  const [contentReads, setContentReads] = useState<Record<string, DiscoveryContentRead>>({});
  const [contentReadError, setContentReadError] = useState<string | null>(null);
  useEffect(() => {
    const restored = loadRunCheckpoint(window.localStorage, campaignId);
    latestCheckpoint.current = restored;
    setRunCheckpoint(restored);
  }, [campaignId]);
  const campaign = useQuery({
    queryKey: ["opencli-campaign", campaignId],
    queryFn: () => getDiscoveryCampaign(campaignId!),
    enabled: !!campaignId,
    retry: false,
    refetchOnWindowFocus: true,
    refetchInterval: 5_000,
  });
  useEffect(() => {
    const scopes = campaign.data?.scopes;
    if (scopes?.length && !scopes.some(scope => scope.platform === activePlatform)) {
      setActivePlatform(scopes[0].platform);
    }
  }, [campaign.data?.scopes, activePlatform]);
  const hits = useInfiniteQuery({
    queryKey: ["opencli-campaign-hits", campaignId],
    queryFn: ({ pageParam }) => listDiscoveryHits(campaignId!, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: nextDiscoveryPage,
    enabled: !!campaign.data,
    retry: false,
    refetchOnWindowFocus: true,
    refetchInterval: 30_000,
  });
  const candidates = useInfiniteQuery({
    queryKey: ["opencli-campaign-candidates", campaignId],
    queryFn: ({ pageParam }) => listDiscoveryCandidates(campaignId!, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: nextDiscoveryPage,
    enabled: !!campaign.data,
    retry: false,
    refetchOnWindowFocus: true,
    // MCP can change Campaign records without a Web runtime event yet.
    refetchInterval: 30_000,
  });
  const counts = useQuery({
    queryKey: ["opencli-campaign-candidate-counts", campaignId],
    queryFn: () => countDiscoveryCandidates(campaignId!),
    enabled: !!campaign.data,
    retry: false,
    refetchOnWindowFocus: true,
    refetchInterval: 5_000,
  });
  const librarySyncing = useRef(false);
  const librarySync = useMutation({
    mutationFn: syncDiscoveryLibrary,
    onMutate: () => { librarySyncing.current = true; },
    onSuccess: (result, id) => { queryClient.setQueryData(["discovery-library", credentialId, id], result); setLastTables(previous => ({ ...previous, [id]: result })); setSyncFailure(previous => previous?.campaignId === id ? null : previous); reviewSyncPending.current.delete(id); },
    onError: (error, id) => setSyncFailure({ campaignId: id, key: discoveryLibraryFailureKey(error) }),
    onSettled: () => { librarySyncing.current = false; void queryClient.invalidateQueries({ queryKey: discoveryExecutionKey(credentialId) }); },
  });
  const create = useMutation({
    mutationFn: () => createDiscoveryCampaign({
      id: draftId, brief: brief.trim(), opencli_version: version,
      criteria: { min_subscribers: minSubscribers, region: region.trim(), language: language.trim(),
        contact_preference: "public_email_first", requirements: [{ description: "核实账号符合请求中的创作者角色，并说明实际账号类型及其发布或策展证据。普通评论、消费提问或销售服务不能仅凭主题相关计入；不得添加未要求的原创、频率或所在地条件。", excerpt: brief.trim() }],
        ...(suppliedScopes.some(scope => scope.platform === "pinterest") ? { pinterest_min_monthly_views_exclusive: PINTEREST_MIN_MONTHLY_VIEWS_EXCLUSIVE } : {}),
        ...(category.trim() ? { category: category.trim() } : {}), requires_ai_tool_demo: requireDemo, requires_public_tool_url: requireToolLink },
      scopes: suppliedScopes.map(scope => ({ ...scope, profile_ref: profileRef })),
      target_count: targetCount, max_calls: maxCalls,
    }),
    onSuccess: result => {
      setCampaignId(result.id);
      queryClient.setQueryData(["opencli-campaign", result.id], result);
      try { window.localStorage.setItem(SAVED_CAMPAIGN_KEY, result.id); } catch { /* Current page remains usable. */ }
    },
  });
  const restore = useMutation({
    mutationFn: (id: string) => getDiscoveryCampaign(id),
    onSuccess: result => {
      void queryClient.invalidateQueries({ queryKey: ["discovery-library", credentialId, result.id] });
      queryClient.setQueryData(["opencli-campaign", result.id], result);
      try { window.localStorage.setItem(SAVED_CAMPAIGN_KEY, result.id); } catch { /* Current page still resumes. */ }
      setCampaignId(result.id);
      setRestoreId("");
      setRestoreOpen(false);
    },
  });
  const approve = useMutation({
    retry: false,
    mutationFn: (_startAfterApproval: boolean) => approveDiscoveryCampaign(campaign.data!.id, campaign.data!.revision),
    onSuccess: (result, startAfterApproval) => {
      queryClient.setQueryData(["opencli-campaign", result.id], result);
      if (startAfterApproval) void startAutomatic(result);
    },
    onError: () => { void campaign.refetch(); },
  });
  // Launch only after the newly saved Campaign is selected. Starting inside the
  // prepare callback would be cancelled by the old Campaign's effect cleanup.
  useEffect(() => {
    if (!pendingAutoApproval || campaignId !== pendingAutoApproval.id || !campaign.data) return;
    setPendingAutoApproval(null);
    if (!canStartPreparedDraft(pendingAutoApproval, campaign.data, campaignId, profileRef) || consumedAutoApprovals.current.has(pendingAutoApproval.id)) return;
    consumedAutoApprovals.current.add(pendingAutoApproval.id);
    approve.mutate(true);
  }, [pendingAutoApproval, campaignId, campaign.data, profileRef, approve.mutate]);
  const verify = useMutation({
    mutationFn: (hitId: string) => verifyDiscoveryYoutubeHitOnce(campaign.data!.id, campaign.data!.revision, profileRef, hitId),
    onSuccess: result => {
      queryClient.setQueryData(["opencli-campaign", result.id], result);
      void queryClient.invalidateQueries({ queryKey: ["opencli-campaign", result.id] });
      void queryClient.invalidateQueries({ queryKey: ["opencli-campaign-candidates", result.id] });
      void queryClient.invalidateQueries({ queryKey: ["opencli-campaign-candidate-counts", result.id] });
    },
    onError: () => { void campaign.refetch(); void candidates.refetch(); void counts.refetch(); },
  });
  const recoverSearch = useMutation({
    mutationFn: (input: { id: string; revision: number; platform: string; profileRef: string; recovery?: { expected_stop_code: string; expected_stopped_at: number } }) =>
      searchDiscoveryOnce(input.id, input.revision, input.platform, input.profileRef, input.recovery),
    retry: false,
    onSuccess: result => queryClient.setQueryData(["opencli-campaign", result.id], result),
    onSettled: (_result, _error, input) => Promise.all([
      queryClient.invalidateQueries({ queryKey: ["opencli-campaign", input.id] }),
      queryClient.invalidateQueries({ queryKey: ["opencli-campaign-hits", input.id] }),
    ]),
  });
  const suppliedScopes = discoveryEditedScopes(platforms.map(platform => ({ platform,
    keywords: discoveryScopeKeywords(platform, keywords, v2exNodes, redditCommunities, tagInputs, categoryInputs) })),
    editingFrom ? queryClient.getQueryData<DiscoveryCampaign>(["opencli-campaign", editingFrom])?.scopes : undefined, keywords);
  const completeScopes = suppliedScopes.every(scope => scope.keywords.length > 0);
  const valid = brief.trim() && availablePlatforms.length > 0 && (!platformsEdited || platforms.length > 0) && !planningBriefTooLong
    && platforms.every(platform => availablePlatforms.includes(platform))
    && suppliedScopes.every(scope => scope.keywords.length === 0 || validDiscoveryScope(scope.platform, scope.keywords))
    && Number.isInteger(targetCount) && targetCount > 0 && targetCount <= 10000 && (maxCalls === null || (Number.isInteger(maxCalls) && maxCalls >= platforms.length && maxCalls <= 10000)) && minSubscribers >= 0;
  const preparePlan = (startAfterPrepare = false) => {
    const previous = editingFrom ? queryClient.getQueryData<DiscoveryCampaign>(["opencli-campaign", editingFrom]) : undefined;
    aiDraft.mutate({ id: draftId,
    brief: planningBrief, profileRef, version, platforms: platformsEdited ? platforms : availablePlatforms,
    targetCount: previous && targetCount === previous.target_count ? null : targetEdited ? targetCount : null,
    maxCalls: null, readBudget: { mode: budgetEdited ? "explicit" : "default", limit: maxCalls }, startAfterPrepare,
    refinement: previous ? { campaign_id: previous.id, expected_revision: previous.revision } : undefined,
    minFollowersOverride: followersEdited ? minSubscribers : null,
    scopeOverrides: discoveryPlanningScopeOverrides(suppliedScopes),
    criteriaOverrides: { region: "",
      language: previous && language === (previous.criteria.language ?? "") ? null : language.trim() || (editingFrom ? "" : null),
      category: previous && category === (previous.criteria.category ?? "") ? null : category.trim() || null,
      requires_ai_tool_demo: requireDemo, requires_public_tool_url: requireToolLink } });
  };
  const current = campaign.data;
  const currentExecution = activeExecution?.revision === current?.revision ? activeExecution : null;
  useEffect(() => {
    if (!currentExecution) return;
    for (const key of ["opencli-campaign", "opencli-campaign-hits", "opencli-campaign-candidates", "opencli-campaign-candidate-counts"]) {
      void queryClient.invalidateQueries({ queryKey: [key, currentExecution.campaign_id] });
    }
  }, [queryClient, currentExecution?.updated_at, currentExecution?.campaign_id]);
  // Coverage describes the frozen Campaign, including sources awaiting login.
  const visibleRuns = current?.platform_runs ?? [];
  const coverage = current && {
    total: visibleRuns.length,
    attempted: visibleRuns.filter(item => item.status !== "unsearched").length,
    completed: visibleRuns.filter(item => item.status === "completed").length,
    unauthorized: visibleRuns.filter(item => item.status === "unauthorized").length,
    technical: visibleRuns.filter(item => item.status === "technical_stop").length,
    risk: visibleRuns.filter(item => item.status === "risk_control_stop").length,
  };
  const visibleHits = hits.data?.pages.flat() ?? [];
  const fitOrder = { excellent: 0, good: 1, medium: 2, low: 3 };
  const visibleCandidates = [...(candidates.data?.pages.flat() ?? [])].sort((a, b) => {
    const verdictOrder = { qualified: 0, pending: 1, rejected: 2 };
    const fit = (item: typeof a) => item.verdict !== "pending" && item.evidence.assessment?.match_rating
      ? fitOrder[item.evidence.assessment.match_rating.level] : 4;
    return verdictOrder[a.verdict] - verdictOrder[b.verdict] || fit(a) - fit(b);
  });
  const identityRequest = { campaignId: current?.id ?? "", revision: current?.revision ?? 0 };
  // 审核面板只读取记录；新增推荐、暂停和结束时的同步由调度器负责。
  const authorGroups = useQuery({
    queryKey: authorGroupsKey(identityRequest), queryFn: () => getDiscoveryAuthorGroups(identityRequest),
    enabled: !!current && visibleHits.length > 0 && !running,
    retry: false, staleTime: 0, refetchOnWindowFocus: true,
    refetchInterval: query => authorGroupsRefreshInterval(query.state.dataUpdateCount, query.state.status === "error"),
  });
  const savedLibrary = useQuery({
    queryKey: ["discovery-library", credentialId, current?.id],
    queryFn: () => getDiscoveryLibrary(current!.id), enabled: !!current,
    retry: false, staleTime: 5_000, refetchOnWindowFocus: true,
  });
  const recentLibrarySync = current && librarySync.data?.campaign_id === current.id
    && librarySync.submittedAt > Number(currentExecution?.synced_at ?? 0) ? librarySync.data : null;
  const libraryResult = current ? recentLibrarySync ?? currentExecution?.library ?? lastTables[current.id] ?? savedLibrary.data ?? null : null;
  const verifiedSourceUrls = useMemo(() => new Set(
    (candidates.data?.pages ?? []).flatMap(page => page.flatMap(candidate => candidate.evidence.source_urls)),
  ), [candidates.data]);
  const onVerifyHit = useCallback((hitId: string) => verify.mutate(hitId), [verify.mutate]);
  const run = current?.platform_runs.filter(item => !excludedPlatforms.includes(item.platform)).find(item => item.platform === activePlatform);
  const frozenProfile = current?.scopes[0]?.profile_ref;
  const profileChanged = !!frozenProfile && frozenProfile !== profileRef;

  const startAnother = () => {
    startGeneration.current += 1;
    try { window.localStorage.removeItem(SAVED_CAMPAIGN_KEY); } catch { /* Local selection is still cleared. */ }
    setDraftId(crypto.randomUUID());
    setCampaignId(null);
    setBrief("");
    setExpandedFrom(null);
    setEditingFrom(null);
    aiDraft.reset();
    setKeywords("");
    setV2exNodes("");
    setRedditCommunities("");
    setTagInputs({});
    setCategoryInputs({});
    setPlatforms(availablePlatforms.filter(platform => platform === "youtube"));
    setCategory("");
    setLanguage("");
    setTargetCount(30);
    setTargetEdited(false);
    setPlatformsEdited(false);
    setMaxCalls(DISCOVERY_DEFAULTS.maxCalls);
    setBudgetEdited(false);
    setMinSubscribers(DISCOVERY_DEFAULTS.minSubscribers);
    setFollowersEdited(false);
    setPreferencesStatus(null);
    setPendingAutoApproval(null);
    startExecution.reset();
    setPlatformsOpen(false);
    setRestoreOpen(false);
    setRequireDemo(false);
    setRequireToolLink(false);
    setSyncFailure(null);
  };
  const startExpanded = () => {
    if (!current) return;
    const criterion = (key: string) => current.criteria[key];
    const keywordsFromCurrent = [...new Set(current.scopes.flatMap(scope => scope.keywords))];
    startAnother();
    setExpandedFrom(current.id);
    setBrief(current.brief);
    setKeywords(keywordsFromCurrent.slice(0, 20).join(", "));
    setLanguage(typeof criterion("language") === "string" ? criterion("language") as string : "");
    setCategory(typeof criterion("category") === "string" ? criterion("category") as string : "");
    const threshold = criterion("min_subscribers") ?? criterion("min_followers");
    if (isU64(threshold)) { setMinSubscribers(threshold); setFollowersEdited(true); }
    setRequireDemo(criterion("requires_ai_tool_demo") === true);
    setRequireToolLink(criterion("requires_public_tool_url") === true);
    setTargetCount(current.target_count);
    setTargetEdited(true);
    setPlatforms(availablePlatforms);
    setMaxCalls(null);
    setBudgetEdited(true);
    setV2exNodes(current.scopes.find(scope => scope.platform === "v2ex-node")?.keywords.join(", ") ?? "");
    setRedditCommunities(current.scopes.find(scope => scope.platform === "reddit-subreddit")?.keywords.join(", ") ?? "");
    setTagInputs(Object.fromEntries(current.scopes.filter(scope => TAG_SEARCH_PLATFORMS.some(platform => platform === scope.platform))
      .map(scope => [scope.platform, scope.keywords.join(", ")])));
    setCategoryInputs(Object.fromEntries(current.scopes.filter(scope => CATEGORY_SEARCH_PLATFORMS.some(platform => platform === scope.platform))
      .map(scope => [scope.platform, scope.keywords.join(", ")])));
  };
  const editPlan = () => {
    if (!current || running || current.platform_runs.some(run => run.status === "searching")) return;
    const previous = current;
    startExpanded();
    setExpandedFrom(null);
    setEditingFrom(previous.id);
    setBrief("");
    setPlatforms(previous.scopes.map(scope => scope.platform));
    setPlatformsEdited(true);
    // Old settings may be execution prose. Explicit form values supersede them.
    setRequireDemo(false);
    setRequireToolLink(false);
    setMaxCalls(previous.max_calls);
    setBudgetEdited(false);
  };
  const exportForLibrary = async () => {
    if (!current || exporting) return;
    setExporting(true);
    setExportError(false);
    try {
      const allHits = await loadAllDiscoveryHits(current.id);
      const file = new Blob([discoveryHitsCsv(current.id, allHits)], { type: "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(file);
      const link = document.createElement("a");
      link.href = url;
      link.download = `onecrew-search-${current.id}.csv`;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
    } catch {
      setExportError(true);
    } finally {
      setExporting(false);
    }
  };
  const preflightBusy = useRef(false);
  const [checkingStartLogin, setCheckingStartLogin] = useState(false);
  const startGeneration = useRef(0);
  const startIdentity = useRef({campaignId, profileRef, credentialId});
  startIdentity.current = {campaignId, profileRef, credentialId};
  const startAutomatic = async (approved = current) => {
    if (!approved || approved.phase !== "active" || running || preflightBusy.current || recoverSearch.isPending
      || approved.scopes.some(scope => scope.profile_ref !== profileRef)) return;
    preflightBusy.current = true;
    setCheckingStartLogin(true);
    const generation = startGeneration.current;
    const identity = {campaignId: approved.id, profileRef, credentialId};
    try {
      const excluded = await preflightOpenCliSearch(credentialId, profileRef,
        capabilities.filter(item => approved.scopes.some(scope => scope.platform === item.platform)));
      // Navigating to another campaign or browser identity cancels the pending start.
      if (generation !== startGeneration.current || startIdentity.current.campaignId !== identity.campaignId || startIdentity.current.profileRef !== identity.profileRef
        || startIdentity.current.credentialId !== identity.credentialId) return;
      startExecution.mutate({campaign_id: approved.id, expected_revision: approved.revision,
        profile_ref: profileRef, excluded_platforms: excluded});
    } finally { preflightBusy.current = false; setCheckingStartLogin(false); }
  };

  const stage = !running && current && counts.data && counts.data.qualified >= current.target_count ? "finished"
    : currentExecution?.stage ?? discoveryStage(current, executionRunning(activeExecution), pausing,
      runCheckpoint && (runCheckpoint.campaign_id !== current?.id || runCheckpoint.revision !== current?.revision) ? "" : executionStatus);
  const runHistory = current ? loadRunHistory(window.localStorage, current.id) : [];
  return <Card className="discovery-surface">
    {!current && <CardHeader><CardTitle className="text-2xl">{t("opencliPilot.title")}</CardTitle></CardHeader>}
    <CardContent className="space-y-6">
      <DiscoveryWorkspacePresence campaignId={current?.id ?? null} stage={stage} excludedPlatforms={excludedPlatforms}
        stop={runCheckpoint?.campaign_id === current?.id && runCheckpoint?.revision === current?.revision ? runCheckpoint : null}
        lastStop={() => latestCheckpoint.current} />
      {!current && <p className="text-sm text-muted-foreground">{t("opencliPilot.scope")}</p>}
      <Collapsible open={restoreOpen} onOpenChange={setRestoreOpen} className="rounded-md border p-3"><CollapsibleTrigger asChild><Button variant="ghost">{t("opencliPilot.restoreTitle")}</Button></CollapsibleTrigger><CollapsibleContent className="space-y-2 pt-2">
          <p className="text-xs text-muted-foreground">{t("opencliPilot.restoreHelp")}</p>
          <p className="text-sm font-medium">{t("opencliPilot.recentPlans")}</p>
          {recentCampaigns.isPending && <p className="text-sm" role="status">{t("opencliPilot.recentLoading")}</p>}
          {recentCampaigns.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.recentFailed")} <Button size="sm" variant="outline" onClick={() => { void recentCampaigns.refetch(); }}>{t("opencliPilot.recentRetry")}</Button></AlertDescription></Alert>}
          {recentCampaigns.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("opencliPilot.recentEmpty")}</p>}
          {recentCampaigns.data?.map(plan => <Button key={plan.id} variant="outline" className="h-auto w-full justify-start whitespace-normal text-left" disabled={aiDraft.isPending || create.isPending || restore.isPending} onClick={() => restore.mutate(plan.id)}>
            <span className="min-w-0 space-y-1 py-1"><span className="block font-medium break-words">{plan.brief}</span><span className="block text-xs font-normal text-muted-foreground">{t("opencliPilot.recentDetails", { date: formatDate(plan.updated_at, i18n.language), target: plan.target_count, used: plan.used_calls, budget: plan.max_calls ?? "∞" })}</span></span>
          </Button>)}
          <div className="flex flex-wrap items-end gap-2"><div className="grid gap-1 grow"><Label htmlFor="discovery-restore-id">{t("opencliPilot.restoreId")}<span className="ml-2 text-xs font-normal text-muted-foreground">{t("opencliPilot.restoreHint")}</span></Label><Input id="discovery-restore-id" value={restoreId} placeholder={t("opencliPilot.restorePlaceholder")} maxLength={128} onChange={event => setRestoreId(event.target.value.trim())} /></div>
            <Button variant="outline" disabled={aiDraft.isPending || create.isPending || restore.isPending || !validDiscoveryCampaignId(restoreId)} onClick={() => restore.mutate(restoreId)}>{t("opencliPilot.restore")}</Button></div>
          {restore.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.restoreFailed")}</AlertDescription></Alert>}
        </CollapsibleContent></Collapsible>
      {!campaignId && <div className="grid gap-3">
        {editingFrom && <Alert><AlertDescription>{t("opencliPilot.editPlanHelp")}</AlertDescription></Alert>}
        {expandedFrom && <Alert><AlertDescription>{maxCalls === null
          ? t("opencliPilot.expansionReviewUnlimited", { sources: platforms.length, target: targetCount })
          : t("opencliPilot.expansionReview", { sources: platforms.length, target: targetCount, calls: maxCalls })}</AlertDescription></Alert>}
        <div className="grid gap-1"><Label htmlFor="discovery-brief">{t("opencliPilot.productBrief")}</Label><p id="discovery-product-guide" className="text-xs text-muted-foreground">{t("opencliPilot.productGuide")}</p><Textarea id="discovery-brief" aria-describedby="discovery-product-guide" value={brief} maxLength={4000} disabled={aiDraft.isPending || create.isPending} placeholder={t("opencliPilot.productExample")} onChange={event => {
          const sentence = event.target.value;
          setBrief(sentence);
          setDraftId(crypto.randomUUID());
          if (editingFrom) return;
          setEditingFrom(null);
          setExpandedFrom(null);
          setTargetEdited(false);
          setPlatformsEdited(false);
          setFollowersEdited(false);
          setBudgetEdited(false);
          setMaxCalls(DISCOVERY_DEFAULTS.maxCalls);
          setTargetCount(DISCOVERY_DEFAULTS.targetCount);
          setMinSubscribers(DISCOVERY_DEFAULTS.minSubscribers);
          setPlatforms(availablePlatforms.filter(platform => platform === "youtube"));
          setLanguage("");
          setV2exNodes(""); setRedditCommunities(""); setTagInputs({}); setCategoryInputs({});
          // A new sentence must not keep another product's saved keyword hints.
          setKeywords(""); setCategory("");
        }} /></div>
        <p className="text-sm text-muted-foreground">{t("opencliPilot.defaultsHelp")}</p>
        <Button disabled={!valid || aiDraft.isPending || create.isPending} onClick={() => preparePlan()}>{(aiDraft.isPending || create.isPending) && <Loader2 className="size-4 animate-spin" />}{t("opencliPilot.aiPrepare")}</Button>
        {create.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.createFailed")}</AlertDescription></Alert>}
        {planningBriefTooLong && <p role="alert">{t("opencliPilot.briefTooLong")}</p>}
        {aiDraft.isPending && <p role="status" className="text-sm">{draftProgress.data ? t(`opencliPilot.planningPhases.${draftProgress.data.phase}`) : t("opencliPilot.planningPhases.checking_connection")} · {t("opencliPilot.aiWaitingTimed", { seconds: planningSeconds, count: platforms.length })}</p>}
        {aiDraft.isError && <Alert variant="destructive"><AlertDescription>
          <p>{t(`opencliPilot.${discoveryDraftFailureKey(aiDraft.error)}`)}</p>
          {discoveryDraftFailureKey(aiDraft.error) === "aiUnsupported" && <>
            {!!draftProgress.data?.unsupported_conditions.length && <ul className="my-2 list-disc pl-5">
              {draftProgress.data.unsupported_conditions.map(condition => <li key={condition}>{condition}</li>)}
            </ul>}
            <p>{t("opencliPilot.aiUnsupportedRecovery")}</p>
          </>}
        </AlertDescription></Alert>}
        {aiDraft.isError && <p className="text-xs">{t("opencliPilot.aiRequestId", { id: draftId })}</p>}
        <Collapsible><CollapsibleTrigger asChild><Button variant="secondary" className="h-11 w-full justify-between border text-base font-semibold" disabled={aiDraft.isPending || create.isPending}><span className="flex items-center gap-2"><Settings2 className="size-4" />{t("opencliPilot.advancedSettings")}</span><ChevronDown className="size-4" /></Button></CollapsibleTrigger><CollapsibleContent className="grid gap-3 pt-3"><fieldset disabled={aiDraft.isPending || create.isPending} className="grid gap-3">
        <p className="text-xs text-muted-foreground">{t(expandedFrom ? "opencliPilot.criteriaGuideReview" : "opencliPilot.criteriaGuide")}</p>
        <div className="grid gap-1"><Label htmlFor="discovery-language">{t("opencliPilot.language")}</Label><Input id="discovery-language" placeholder={t("opencliPilot.any")} value={language} onChange={event => setLanguage(event.target.value)} /></div>
        <div className="grid gap-1"><Label htmlFor="discovery-category">{t("opencliPilot.category")}</Label><p className="text-xs text-muted-foreground">{t("opencliPilot.categoryHelp")}</p><Input id="discovery-category" placeholder={t("opencliPilot.autoCategory")} value={category} maxLength={160} onChange={event => setCategory(event.target.value)} /></div>
        <div className="grid gap-1 sm:grid-cols-2 sm:gap-3"><div><Label htmlFor="discovery-followers">{t("opencliPilot.minSubscribers")}</Label><Input id="discovery-followers" type="number" min={0} placeholder="2000" value={String(followersInput)} onChange={event => { const value = event.target.value; const parsed = creatorU64.safeParse(/^\d+$/.test(value) ? BigInt(value) : Number(value)); setMinSubscribers(value === "" ? "" : parsed.success ? parsed.data : NaN); setFollowersEdited(value !== ""); }} /></div><div><Label htmlFor="discovery-target">{t("opencliPilot.target")}</Label><Input id="discovery-target" type="number" min={1} value={targetInput} onChange={event => { setTargetEdited(true); setTargetCount(event.target.value === "" ? "" : Number(event.target.value)); }} /></div></div>
        <div><Label htmlFor="discovery-max-calls">{t("opencliPilot.maxCalls")}</Label><Input id="discovery-max-calls" type="number" min={Math.max(1, platforms.length)} max={10000} step={1} value={maxCalls ?? ""} aria-describedby="discovery-budget-help" onChange={event => { setBudgetEdited(true); setMaxCalls(event.target.value === "" ? null : Number(event.target.value)); }} /><p id="discovery-budget-help" className="text-xs text-muted-foreground">{t("opencliPilot.maxCallsHelp")}</p></div>
        <p className="text-xs text-muted-foreground">{t("opencliPilot.criteriaWarning")}</p>
        {platforms.some(platform => platform !== "v2ex-node" && platform !== "reddit-subreddit" && !TAG_SEARCH_PLATFORMS.some(source => source === platform) && !CATEGORY_SEARCH_PLATFORMS.some(source => source === platform)) && <div className="grid gap-1"><Label htmlFor="discovery-keywords">{t("opencliPilot.keywords")}</Label><p className="text-xs text-muted-foreground">{t("opencliPilot.keywordsHelp")}</p><Input id="discovery-keywords" value={keywords} onChange={event => setKeywords(event.target.value)} placeholder={t("opencliPilot.keywordExample")} /></div>}
        <Collapsible><CollapsibleTrigger asChild><Button variant="secondary" className="h-11 w-full justify-between border text-base font-semibold"><span className="flex items-center gap-2"><Settings2 className="size-4" />{t("opencliPilot.manualSources")}</span><ChevronDown className="size-4" /></Button></CollapsibleTrigger><CollapsibleContent className="grid gap-3 pt-3">
        <p className="text-xs text-muted-foreground">{t("opencliPilot.manualSourcesHelp")}</p>
        <p className="text-sm text-muted-foreground">{t("opencliPilot.optionalSourceHelp")}</p>
        <div className="grid gap-3"><Label>{t("opencliPilot.platforms")}</Label><p className="text-xs text-muted-foreground">{t("opencliPilot.platformAccess", { selected: platforms.length, available: availablePlatforms.length })}</p>{SEARCH_PLATFORM_GROUPS.map(group => {
          const supported = group.platforms.filter(platform => availablePlatforms.includes(platform));
          if (!supported.length) return null;
          const selectable = supported.filter(platform => !excludedPlatforms.includes(platform));
          const allSelected = selectable.length > 0 && selectable.every(platform => platforms.includes(platform));
          return <fieldset key={group.label} className="space-y-2">
            <legend className="flex flex-wrap items-center gap-3 text-sm font-medium">{t(`opencliPilot.platformGroups.${group.label}`)}
              <Button type="button" size="sm" variant="outline" disabled={!selectable.length}
                onClick={() => { setPlatformsEdited(true); setPlatforms(previous => allSelected ? previous.filter(platform => !supported.includes(platform as typeof supported[number])) : [...new Set([...previous, ...selectable])]); }}>
                {t(allSelected ? "opencliPilot.deselectGroup" : "opencliPilot.selectGroup")}
              </Button>
            </legend>
            <div className="flex flex-wrap gap-x-4 gap-y-2">{supported.map(platform => <label key={platform} className="flex items-center gap-2 text-sm">
              <Checkbox checked={platforms.includes(platform)} disabled={excludedPlatforms.includes(platform)}
                onCheckedChange={checked => { setPlatformsEdited(true); setPlatforms(previous => checked ? [...new Set([...previous, platform])] : previous.filter(item => item !== platform)); }} />{platformDisplayName(platform)}
            </label>)}</div>
          </fieldset>;
        })}</div>
        {selectedBrowserPlatforms.length > 0 && <Alert><AlertDescription>{t("opencliPilot.accountSetup", { platforms: browserPlatformSummary })}</AlertDescription></Alert>}
        {platforms.includes("pinterest") && <DiscoveryPlanRequirements criteria={{ min_subscribers: minSubscribers, pinterest_min_monthly_views_exclusive: PINTEREST_MIN_MONTHLY_VIEWS_EXCLUSIVE }} />}
        {platforms.some(platform => platform === "v2ex-node" || platform === "reddit-subreddit" || TAG_SEARCH_PLATFORMS.some(source => source === platform) || CATEGORY_SEARCH_PLATFORMS.some(source => source === platform)) && <p className="text-xs text-muted-foreground">{t("opencliPilot.scopedSourcesHelp")}</p>}
        {platforms.includes("v2ex-node") && <div className="grid gap-1"><Label htmlFor="discovery-v2ex-nodes">{t("opencliPilot.v2exNodes")}</Label><Input id="discovery-v2ex-nodes" value={v2exNodes} onChange={event => setV2exNodes(event.target.value)} placeholder={t("opencliPilot.v2exNodesExample")} /><p className="text-xs text-muted-foreground">{t("opencliPilot.v2exNodesHelp")}</p></div>}
        {platforms.includes("reddit-subreddit") && <div className="grid gap-1"><Label htmlFor="discovery-reddit-communities">{t("opencliPilot.redditCommunities")}</Label><Input id="discovery-reddit-communities" value={redditCommunities} onChange={event => setRedditCommunities(event.target.value)} placeholder={t("opencliPilot.redditCommunitiesExample")} /><p className="text-xs text-muted-foreground">{t("opencliPilot.redditCommunitiesHelp")}</p></div>}
        {TAG_SEARCH_PLATFORMS.filter(platform => platforms.includes(platform)).map(platform => <div key={platform} className="grid gap-1"><Label htmlFor={`discovery-tags-${platform}`}>{t("opencliPilot.platformTags", { platform })}</Label><Input id={`discovery-tags-${platform}`} value={tagInputs[platform] ?? ""} onChange={event => setTagInputs(current => ({ ...current, [platform]: event.target.value }))} placeholder={t("opencliPilot.platformTagsExample")} /><p className="text-xs text-muted-foreground">{t("opencliPilot.platformTagsHelp")}</p></div>)}
        {CATEGORY_SEARCH_PLATFORMS.filter(platform => platforms.includes(platform)).map(platform => <div key={platform} className="grid gap-1"><Label htmlFor={`discovery-categories-${platform}`}>{t("opencliPilot.platformCategories", { platform })}</Label><Input id={`discovery-categories-${platform}`} value={categoryInputs[platform] ?? ""} onChange={event => setCategoryInputs(current => ({ ...current, [platform]: event.target.value }))} placeholder={platform === "juejin-hot" ? t("opencliPilot.juejinCategoriesExample") : t("opencliPilot.productHuntCategoriesExample")} /><p className="text-xs text-muted-foreground">{platform === "juejin-hot" ? t("opencliPilot.juejinCategoriesHelp") : t("opencliPilot.productHuntCategoriesHelp")}</p></div>)}
        {suppliedScopes.some(scope => scope.keywords.length > 0 && !validDiscoveryScope(scope.platform, scope.keywords)) && <p role="alert" className="text-sm text-destructive">{t("opencliPilot.invalidScopeHelp")}</p>}
        <Button disabled={!valid || !completeScopes || create.isPending || aiDraft.isPending} onClick={() => {
          try { window.localStorage.setItem(SAVED_CAMPAIGN_KEY, draftId); } catch { /* In-memory draft remains usable. */ }
          create.mutate();
        }}>{create.isPending && <Loader2 className="size-4 animate-spin" />}{t("opencliPilot.createManual")}</Button>
        </CollapsibleContent></Collapsible>
        <Button variant="outline" disabled={platforms.length === 0 || targetInput === "" || followersInput === "" || create.isPending || aiDraft.isPending} onClick={() => {
          let saved = false;
          try { saved = saveDiscoveryPreferences(window.localStorage, { format: 1, brief, region, language, category, minSubscribers, followersEdited, targetCount, maxCalls, keywords, v2exNodes, redditCommunities, tagInputs, categoryInputs, platforms, requireDemo, requireToolLink }); } catch { /* Report storage failure without pretending the draft was saved. */ }
          if (!saved) { setPreferencesStatus("failed"); return; }
          if (!brief.trim()) { setPreferencesStatus("needsProduct"); return; }
          if (planningBriefTooLong) { setPreferencesStatus(null); return; }
          setPreferencesStatus("saved");
          if (!valid) { setPreferencesStatus(null); return; }
          preparePlan();
        }}>{t(editingFrom ? "opencliPilot.editSave" : expandedFrom ? "opencliPilot.aiPrepare" : "opencliPilot.savePreferences")}</Button>
        {preferencesStatus && <p role={preferencesStatus === "saved" ? "status" : "alert"} className="text-sm">{t(preferencesStatus === "saved" ? "opencliPilot.preferencesSaved" : preferencesStatus === "needsProduct" ? "opencliPilot.productRequired" : "opencliPilot.preferencesFailed")}</p>}
        </fieldset></CollapsibleContent></Collapsible>
      </div>}
      {campaignId && campaign.isPending && <Loader2 className="size-4 animate-spin" aria-label={t("opencliPilot.loading")} />}
      {campaignId && campaign.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.loadFailed")}</AlertDescription></Alert>}
      {campaignId && !current && <Button variant="outline" onClick={startAnother}>{t("opencliPilot.another")}</Button>}
      {checkingStartLogin && <p role="status" className="flex items-center gap-2 text-sm"><Loader2 className="size-4 animate-spin" aria-hidden />{t("opencli.authCheckAll")}</p>}
      {startExecution.isPending && !executionRunning(activeExecution) && <p role="status" className="flex items-center gap-2 text-sm"><Loader2 className="size-4 animate-spin" aria-hidden />{t("opencliPilot.starting")}</p>}
      {current && <div className="space-y-6">
        {execution.isError && <Alert><AlertDescription>{t("opencliPilot.executionConnectionFailed")}</AlertDescription></Alert>}
        {(startExecution.isError || pauseExecution.isError) && <Alert variant="destructive"><AlertDescription>{t(startExecution.isError && discoveryLibraryFailureKey(startExecution.error) !== "librarySyncFailed" ? `opencliPilot.${discoveryLibraryFailureKey(startExecution.error)}` : "opencliPilot.executionControlFailed")}</AlertDescription></Alert>}
        <DiscoveryOverview campaign={current} counts={counts.data} stage={stage} running={executionRunning(activeExecution)}
          libraryLoading={savedLibrary.isPending} libraryReadFailed={savedLibrary.isError}
          busy={verify.isPending || recoverSearch.isPending || startExecution.isPending || checkingStartLogin} profileChanged={profileChanged} syncing={librarySync.isPending}
          library={libraryResult} syncFailed={currentExecution ? !!currentExecution.sync_error && currentExecution.sync_error !== "library_source_stale" : syncFailure?.campaignId === current.id}
          syncPending={currentExecution?.sync_pending ?? false} syncedAt={currentExecution?.synced_at ?? null}
          syncErrorKey={currentExecution?.sync_error ? discoveryLibraryFailureKey(new Error(currentExecution.sync_error)) : syncFailure?.campaignId === current.id ? syncFailure.key : undefined}
          failurePhase={currentExecution?.failure_phase ?? undefined} failureCode={currentExecution?.failure_code ?? undefined}
          deferredHits={currentExecution?.deferred_hits ?? []}
          excludedSources={(currentExecution?.excluded_platforms ?? []).filter(platform => current.scopes.some(scope => scope.platform === platform)).map(platform => {
            const observation = loadOpenCliAuthObservations(credentialId, profileRef)[platform];
            const reason = !observation ? "unchecked" : observation.issue === "entitlement_unverified" ? "permission" : observation.status === "not_logged_in" ? "signedOut" : observation.status === "error" || observation.issue ? "failed" : observation.method !== "full" || observation.status !== "logged_in" ? "unverified" : "expired";
            return { name: platformDisplayName(platform), reason };
          })}
          onFailure={() => { setPlatformsOpen(true); requestAnimationFrame(() => platformDetails.current?.scrollIntoView({ block: "center", behavior: "smooth" })); }}
          onSync={() => librarySync.mutate(current.id)} onStart={() => { void startAutomatic(); }}
          onPause={() => { startGeneration.current += 1; pauseExecution.mutate(current.id); }} onAnother={startAnother} onExpanded={startExpanded}
          canExpand={availablePlatforms.some(platform => !current.scopes.some(scope => scope.platform === platform))}
          onIssues={() => { const issue = current.platform_runs.find(run => !excludedPlatforms.includes(run.platform) && ["unauthorized", "technical_stop", "risk_control_stop"].includes(run.status)); if (issue) setActivePlatform(issue.platform); setPlatformsOpen(true); requestAnimationFrame(() => platformDetails.current?.scrollIntoView({ block: "start", behavior: "smooth" })); }} />
        {!running && stage !== "searching" && runCheckpoint?.campaign_id === current.id && runCheckpoint.revision === current.revision && runCheckpoint.reason && <p role="status" className="text-sm text-muted-foreground">
          {t("opencliPilot.lastStop")} {t(`opencliPilot.stopReasons.${runCheckpoint.reason}`)} · {formatDate(runCheckpoint.recorded_at / 1000, i18n.resolvedLanguage)}
        </p>}
        {checkpointFailed && <p role="alert" className="text-sm text-destructive">{t("opencliPilot.stopRecordFailed")}</p>}
        {runHistory.length > 0 && <details className="text-sm text-muted-foreground">
          <summary className="cursor-pointer">{t("opencliPilot.stopHistory")}</summary>
          <ul className="space-y-2 pt-2">{runHistory.map((item, index) => <li key={`${item.recorded_at}-${index}`}>
            {formatDate(item.recorded_at / 1000, i18n.resolvedLanguage)} · {item.reason && t(`opencliPilot.stopReasons.${item.reason}`)} · {item.used}/{item.max}
          </li>)}</ul>
        </details>}
        {execution.isError && <Alert><AlertDescription>{t("opencliPilot.statusDisconnected")}</AlertDescription></Alert>}
        {counts.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.candidatesFailed")}</AlertDescription></Alert>}
        {current.phase === "draft" && <div className="rounded-md border p-3 text-sm"><p className="font-medium">{t("opencliPilot.approvalBounds")}</p><p>{current.scopes.map(scope => platformDisplayName(scope.platform)).join(", ")}</p><p>{t("opencliPilot.frozenThreshold", { value: String(current.criteria.min_subscribers ?? current.criteria.min_followers ?? 0) })}</p>{current.criteria.region && <p>{t("opencliPilot.frozenRegion", { value: current.criteria.region })}</p>}<p>{t("opencliPilot.frozenLanguage", { value: current.criteria.language || t("opencliPilot.any") })}</p><p>{t("opencliPilot.frozenCategory", { value: current.criteria.category || t("opencliPilot.any") })}</p>{current.scopes.map(scope => <p key={`${scope.platform}:${scope.profile_ref}`}>{t("opencliPilot.frozenKeywords", { value: `${platformDisplayName(scope.platform)}: ${scope.keywords.join(", ")}` })}</p>)}<p>{t("opencliPilot.frozenBudget", { target: current.target_count, calls: current.max_calls ?? "∞", version: current.opencli_version })}</p></div>}
        <DiscoveryPlanRequirements criteria={current.criteria} />
        {current.phase === "active" && <Button variant="secondary" className="h-11 border font-semibold" disabled={running || pausing || verify.isPending || recoverSearch.isPending || current.platform_runs.some(run => run.status === "searching")} onClick={editPlan}><Settings2 className="size-4" />{t("opencliPilot.editPlan")}</Button>}
        {current.phase === "active" && <Collapsible className="discovery-disclosure"><CollapsibleTrigger asChild><Button variant="ghost">{t("opencliPilot.adjustPlan")}</Button></CollapsibleTrigger>
          <CollapsibleContent className="space-y-3 pb-4">
          {running && <p role="status" className="text-sm text-muted-foreground">{t("opencliPilot.pauseBeforeAdjust")}</p>}
          <DiscoveryTargetIncrease key={current.id} campaign={current} busy={running || verify.isPending} />

        </CollapsibleContent></Collapsible>}
        {current.criteria.automatic_query_expansion === true && <p className="text-sm text-muted-foreground">{t("opencliPilot.automaticQueries")}</p>}
        {current.phase === "draft" && <div className="flex flex-wrap gap-2">
          <Button disabled={approve.isPending || profileChanged} onClick={() => approve.mutate(true)}>{approve.isPending && <Loader2 className="size-4 animate-spin" />}{t("opencliPilot.approveAndStart")}</Button>
          <Button variant="outline" disabled={approve.isPending || profileChanged} onClick={() => approve.mutate(false)}>{t("opencliPilot.approveOnly")}</Button>
        </div>}
        {platformsOpen && <Collapsible ref={platformDetails} key={`platform-runs:${current.id}`} open={platformsOpen} onOpenChange={setPlatformsOpen} className="discovery-disclosure">
        <CollapsibleTrigger asChild><Button variant="ghost">{t("opencliPilot.platformRunsDetails")}</Button></CollapsibleTrigger>
        <CollapsibleContent className="space-y-3 pt-3">
        {current.phase === "active" && <div className="grid gap-1"><Label htmlFor="discovery-active-platform">{t("opencliPilot.platformStatusFilter")}</Label><select id="discovery-active-platform" className="h-9 rounded-md border bg-background px-3 text-sm" value={activePlatform} onChange={event => setActivePlatform(event.target.value)}>{current.scopes.filter(scope => !excludedPlatforms.includes(scope.platform)).map(scope => <option key={scope.platform} value={scope.platform}>{platformDisplayName(scope.platform)}</option>)}</select></div>}
        {run && <p className="text-sm text-muted-foreground">{t("opencliPilot.runStatus", { status: discoveryRunStageKey(run.status) ? t(discoveryRunStageKey(run.status)!) : run.status, calls: run.calls_used })}{run.stop_code ? ` · ${t(discoveryProviderFailureKey(run.stop_code))}` : ""}</p>}
        {coverage && <p className="text-sm text-muted-foreground">{t("opencliPilot.coverage", coverage)}</p>}
        <div className="space-y-3 text-sm">{current.platform_runs.map(item => <div key={`${item.platform}:${item.profile_ref}`} className="flex flex-wrap justify-between gap-2 border-b pb-3">
          <p className="font-medium">{platformDisplayName(item.platform)}</p><p>{discoveryRunStageKey(item.status) ? t(discoveryRunStageKey(item.status)!) : item.status} · {item.calls_used}</p>
          {item.stop_code && <p className="basis-full break-words text-sm text-muted-foreground">{t(discoveryProviderFailureKey(item.stop_code))}</p>}
        </div>)}</div>
        {run?.status === "technical_stop" && <Alert><AlertDescription>{t(run.stop_code === "interrupted_read" ? "opencliPilot.interruptedSearchHelp" : "opencliPilot.technicalStop")}</AlertDescription></Alert>}
        {run?.status === "technical_stop" && canRecoverDiscoverySearch(current, profileRef, activePlatform) && <Button variant="outline"
          disabled={running || verify.isPending || recoverSearch.isPending || !canRecoverDiscoverySearch(current, profileRef, activePlatform)}
          onClick={() => {
            if (running || verify.isPending || recoverSearch.isPending || !canRecoverDiscoverySearch(current, profileRef, activePlatform)) return;
            recoverSearch.mutate({ id: current.id, revision: current.revision, platform: activePlatform, profileRef, ...(run.stop_code && run.stop_code !== "interrupted_read" && typeof run.updated_at === "number" ? { recovery: { expected_stop_code: run.stop_code, expected_stopped_at: run.updated_at } } : {}) });
          }}>
          {recoverSearch.isPending && <Loader2 className="size-4 animate-spin" />}{t(run.stop_code === "interrupted_read" ? "opencliPilot.recoverInterruptedSearch" : "opencliPilot.recoverRepairedSearch", { platform: activePlatform })}
        </Button>}
        {recoverSearch.isError && recoverSearch.variables.id === current.id && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.stepFailed")}</AlertDescription></Alert>}
        {run?.status === "unauthorized" && <Alert><AlertDescription>{t("opencliPilot.accountRequired", { platform: activePlatform })}</AlertDescription></Alert>}
        {run?.status === "risk_control_stop" && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.riskStop")}</AlertDescription></Alert>}
        </CollapsibleContent>
        </Collapsible>}
        {(approve.isError || verify.isError) && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.stepFailed")}</AlertDescription></Alert>}

        {hits.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.hitsFailed")}</AlertDescription></Alert>}
        <details className="discovery-disclosure">
          <summary>{t("opencliPilot.planDetails")}</summary>
          <div className="space-y-2 py-4 text-sm text-muted-foreground break-words">
            <p>{t("opencliPilot.frozenThreshold", { value: String(current.criteria.min_subscribers ?? current.criteria.min_followers ?? 0) })}</p>


            <p>{t("opencliPilot.frozenBudget", { target: current.target_count, calls: current.max_calls ?? "∞", version: current.opencli_version })}</p>
            {current.criteria.region && <p>{t("opencliPilot.frozenRegion", { value: current.criteria.region })}</p>}<p>{t("opencliPilot.frozenLanguage", { value: current.criteria.language || t("opencliPilot.any") })}</p>
            <p>{t("opencliPilot.frozenCategory", { value: current.criteria.category || t("opencliPilot.any") })}</p>
            <p>{t("opencliPilot.planSources", { value: current.scopes.map(scope => platformDisplayName(scope.platform)).join(", ") })}</p>
            {[...new Set(current.scopes.map(scope => JSON.stringify(scope.keywords)))].map(group => {
              const sources = current.scopes.filter(scope => JSON.stringify(scope.keywords) === group).map(scope => scope.platform);
              return <p key={group}>{t("opencliPilot.frozenKeywords", { value: `${sources.map(platformDisplayName).join(", ")}: ${(JSON.parse(group) as string[]).join(", ")}` })}</p>;
            })}
          </div>
        </details>
      </div>}
    </CardContent>
  </Card>;
}
