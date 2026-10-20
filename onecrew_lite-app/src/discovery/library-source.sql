WITH bindings AS (
 SELECT h.id AS hit_id,b.native_author_id,b.candidate_id,
   json_object('video_hit_id',b.search_hit_id,'native_author_id',b.native_author_id,
     'candidate_id',b.candidate_id,'criteria_revision',b.criteria_revision) AS identity_receipt
 FROM discovery_search_hits h
 JOIN discovery_video_completions done ON done.search_hit_id=h.id
 JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id
 WHERE h.discovery_campaign_id=?1 AND h.platform='youtube'
   AND b.discovery_campaign_id=h.discovery_campaign_id AND b.criteria_revision=?2
 UNION ALL
 SELECT h.id,b.native_author_id,b.candidate_id,
   json_object('web_search_hit_id',h.id,'origin_platform',h.platform,'target_platform',web.target_platform,
     'video_id',web.video_id,'native_author_id',b.native_author_id,'candidate_id',b.candidate_id,
     'criteria_revision',b.criteria_revision,'call_number',web.call_number) AS identity_receipt
 FROM discovery_search_hits h
 JOIN discovery_web_source_reads web ON web.search_hit_id=h.id AND web.discovery_campaign_id=h.discovery_campaign_id
   AND web.origin_platform=h.platform AND web.profile_ref=h.profile_ref AND web.source_url=h.source_url
   AND web.expected_revision=?2 AND web.target_platform IN ('youtube','twitter','douyin','instagram','tiktok') AND web.status='completed' AND web.stop_code IS NULL
   AND json_extract(web.native_material_json,CASE web.target_platform WHEN 'twitter' THEN '$.identity.account_id' WHEN 'douyin' THEN '$.content.author_id' WHEN 'instagram' THEN '$.profile.id' WHEN 'tiktok' THEN '$.profile.id' ELSE '$.channelId' END)=web.native_author_id
   AND json_extract(web.native_material_json,CASE web.target_platform WHEN 'twitter' THEN '$.identity.post_id' WHEN 'douyin' THEN '$.content.post_id' WHEN 'instagram' THEN '$.profile.username' WHEN 'tiktok' THEN '$.profile.username' ELSE '$.videoId' END)=web.video_id
   AND (web.target_platform NOT IN ('instagram','tiktok') OR json_extract(web.native_material_json,'$.identity')=json_extract(web.native_material_json,'$.profile'))
 JOIN discovery_video_completions done ON done.search_hit_id=h.id
 JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=h.discovery_campaign_id
   AND b.criteria_revision=?2 AND b.native_author_id=web.native_author_id
 WHERE h.discovery_campaign_id=?1 AND (h.platform IN ('google','yahoo') OR h.platform='instagram' AND web.target_platform='instagram')
 UNION ALL
 SELECT h.id,
   CASE p.platform WHEN 'twitter' THEN m.account_id WHEN 'linkedin' THEN json_extract(p.native_profile_json,'$.account_id') ELSE p.author_id END,NULL,
   p.native_profile_json
 FROM discovery_search_hits h
 JOIN discovery_search_authors a ON a.search_hit_id=h.id
 JOIN discovery_profile_reads p ON p.search_hit_id=h.id
 LEFT JOIN discovery_profile_identity_matches m ON m.search_hit_id=h.id
 LEFT JOIN discovery_identity_reads i ON i.search_hit_id=m.identity_search_hit_id
 WHERE h.discovery_campaign_id=?1 AND p.discovery_campaign_id=h.discovery_campaign_id
   AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.expected_revision=?2
   AND p.status='completed' AND p.author_id=a.author_id
   AND json_extract(p.native_profile_json,CASE WHEN p.platform='zhihu' THEN '$.author_url' ELSE '$.profile_url' END)=a.author_url
   AND (
    (p.platform='bilibili' AND json_extract(p.native_profile_json,'$.mid')=a.author_id)
    OR (p.platform IN ('tiktok','reddit','facebook','instagram','weibo') AND json_extract(p.native_profile_json,'$.id')=a.author_id)
    OR (p.platform IN ('zhihu','hackernews','pinterest','pixiv') AND json_extract(p.native_profile_json,'$.author_id')=a.author_id)
    OR (p.platform='douyin' AND json_extract(p.native_profile_json,'$.author_id')=a.author_id
      AND EXISTS(SELECT 1 FROM discovery_douyin_author_reads joint WHERE joint.search_hit_id=h.id
        AND joint.discovery_campaign_id=h.discovery_campaign_id AND joint.expected_revision=?2
        AND joint.platform=h.platform AND joint.profile_ref=h.profile_ref AND joint.source_url=h.source_url
        AND joint.status='completed' AND joint.stop_code IS NULL AND joint.duplicate_of IS NULL
        AND joint.author_id=a.author_id AND joint.profile_url=a.author_url AND joint.call_number=p.call_number))
    OR (p.platform='bluesky' AND json_extract(p.native_profile_json,'$.did')=a.author_id)
    OR (p.platform='linkedin' AND json_extract(p.native_profile_json,'$.author_handle')=a.author_id
      AND json_extract(p.native_profile_json,'$.evidence_source')='visible_profile_header'
      AND h.source_url=a.author_url AND json_extract(p.native_profile_json,'$.account_id') IS NOT NULL)
    OR (p.platform='twitter' AND json_extract(p.native_profile_json,'$.handle')=a.author_id
      AND json_extract(p.native_profile_json,'$.identity_kind')='native_id'
      AND json_extract(p.native_profile_json,'$.account_id')=m.account_id
      AND i.discovery_campaign_id=h.discovery_campaign_id AND i.platform=h.platform
      AND i.profile_ref=h.profile_ref AND i.expected_revision=?2 AND i.status='completed'
      AND i.author_handle=a.author_id AND i.source_url=h.source_url
      AND i.account_id=m.account_id AND i.call_number<p.call_number)
   )
)
SELECT h.id,h.platform,h.profile_ref,h.search_keyword,h.source_url,h.title,
 CASE WHEN h.platform IN ('google','yahoo') AND c.id IS NOT NULL THEN c.display_name ELSE h.author_label END AS author_label,
 h.captured_at,COALESCE(a.author_id,CASE WHEN h.platform IN ('google','yahoo') THEN c.platform_account_id END) AS author_id,
 COALESCE(a.author_url,CASE WHEN h.platform IN ('google','yahoo') THEN c.profile_url END) AS author_url,
 COALESCE(a.observed_at,CASE WHEN h.platform IN ('google','yahoo') THEN c.created_at END) AS author_observed_at,
 c.id AS candidate_id,c.platform_account_id AS native_author_id,c.verdict,c.evidence_json,b.identity_receipt,manual.approved_at AS manual_approved_at,c.platform AS native_platform
FROM discovery_search_hits h
LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id
LEFT JOIN bindings b ON b.hit_id=h.id
LEFT JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id
 AND ((h.platform IN ('google','yahoo') AND c.platform IN ('youtube','twitter','douyin','instagram','tiktok') AND c.id=b.candidate_id)
   OR (c.platform=h.platform AND c.source_platform=h.platform AND c.source_profile_ref=h.profile_ref))
 AND c.criteria_revision=?2 AND c.platform_account_id=b.native_author_id
 AND (b.candidate_id IS NULL OR c.id=b.candidate_id)
LEFT JOIN discovery_manual_approvals manual ON manual.discovery_campaign_id=c.discovery_campaign_id
 AND manual.criteria_revision=c.criteria_revision AND manual.candidate_id=c.id
WHERE h.discovery_campaign_id=?1
ORDER BY h.id LIMIT 10001
