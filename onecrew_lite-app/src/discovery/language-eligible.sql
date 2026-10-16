SELECT EXISTS(
 SELECT 1 FROM discovery_candidates c JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
 JOIN discovery_search_hits h ON h.discovery_campaign_id=d.id AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
 JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=d.id AND v.revision=d.current_revision
 JOIN discovery_evidence_reviews review ON review.id=json_extract(c.evidence_json,'$.reviewed_match.review_id') AND review.candidate_id=c.id AND review.criteria_revision=d.current_revision
   AND review.discovery_campaign_id=d.id AND review.search_hit_id=h.id
   AND review.policy_version='initial-match-evaluation-v1'
   AND review.material_hash=json_extract(c.evidence_json,'$.reviewed_match.material_hash')
   AND review.native_author_id=json_extract(c.evidence_json,'$.reviewed_match.native_author_id')
 WHERE d.id=?1 AND h.id=?2 AND h.platform IN ('twitter','youtube','bilibili','tiktok','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','linkedin','douyin','google','yahoo') AND c.verdict='pending'
   AND c.criteria_revision=d.current_revision AND d.phase='active' AND v.approved_at IS NOT NULL
   AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs busy WHERE busy.discovery_campaign_id=d.id AND busy.status='searching')
   AND json_extract(v.criteria_json,'$.language') IS NOT NULL
   AND json_extract(c.evidence_json,'$.reviewed_match.outcome')='insufficient'
   AND (
     (h.platform IN ('google','yahoo') AND c.platform IN ('youtube','twitter','douyin') AND c.source_platform=h.platform
       AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=review.native_author_id
       AND json_extract(review.material_json,'$.native_author_id')=review.native_author_id
       AND json_extract(review.material_json,'$.native_platform')=c.platform
       AND json_extract(review.material_json,'$.hit.platform')=h.platform
       AND json_extract(review.material_json,'$.hit.profile_ref')=h.profile_ref
       AND json_extract(review.material_json,'$.hit.source_url')=h.source_url
       AND EXISTS(SELECT 1 FROM discovery_web_source_reads web
         JOIN discovery_video_identity_bindings binding ON binding.search_hit_id=web.search_hit_id
         WHERE web.search_hit_id=h.id AND web.discovery_campaign_id=d.id
           AND web.expected_revision=d.current_revision AND web.origin_platform=h.platform
           AND web.profile_ref=h.profile_ref AND web.source_url=h.source_url
           AND web.target_platform=c.platform AND web.status='completed' AND web.stop_code IS NULL
           AND web.native_author_id=review.native_author_id
           AND binding.discovery_campaign_id=d.id AND binding.criteria_revision=d.current_revision
           AND binding.candidate_id=c.id AND binding.native_author_id=web.native_author_id
           AND json_extract(web.native_material_json,CASE WHEN web.target_platform IN ('twitter','douyin') THEN '$.content.text' ELSE '$.description' END)=json_extract(review.material_json,'$.video_description'))
       AND NOT EXISTS(SELECT 1 FROM discovery_web_source_reads stopped
         WHERE stopped.discovery_campaign_id=d.id AND stopped.profile_ref=h.profile_ref
           AND stopped.target_platform=c.platform AND stopped.stop_code IN ('opencli_auth_required','opencli_risk_control'))
       AND NOT EXISTS(SELECT 1 FROM discovery_language_samples stopped
         JOIN discovery_search_hits sh ON sh.id=stopped.search_hit_id JOIN discovery_candidates sc ON sc.id=stopped.candidate_id
         WHERE stopped.discovery_campaign_id=d.id AND sh.profile_ref=h.profile_ref AND sc.platform=c.platform
           AND stopped.stop_code IN ('opencli_auth_required','opencli_risk_control')))
     OR (h.platform='tiktok'
       AND json_extract(review.material_json,'$.hit.title')=h.title
       AND json_extract(review.material_json,'$.hit.source_url')=h.source_url
       AND json_extract(review.material_json,'$.hit.profile_ref')=h.profile_ref
       AND EXISTS(SELECT 1 FROM discovery_search_authors native WHERE native.search_hit_id=h.id AND native.author_id=review.native_author_id
         AND json_extract(review.material_json,'$.hit.author_id')=native.author_id AND json_extract(review.material_json,'$.hit.author_url')=native.author_url))
     OR (h.platform='youtube'
       AND json_type(review.material_json,'$.video_description')='text'
       AND json_extract(review.material_json,'$.video_description')=json_extract(c.evidence_json,'$.video_description')
       AND (((json_type(review.material_json,'$.transcript') IS NULL OR json_type(review.material_json,'$.transcript')='null')
         AND NOT EXISTS(SELECT 1 FROM discovery_transcript_reads transcript WHERE transcript.search_hit_id=h.id AND transcript.candidate_id=c.id))
         OR EXISTS(SELECT 1 FROM discovery_transcript_reads transcript WHERE transcript.search_hit_id=h.id AND transcript.candidate_id=c.id
           AND json_extract(review.material_json,'$.transcript.search_hit_id')=h.id
           AND json_extract(review.material_json,'$.transcript.candidate_id')=c.id
           AND json_extract(review.material_json,'$.transcript.source_url')=h.source_url
           AND json_extract(review.material_json,'$.transcript.profile_ref')=h.profile_ref
           AND json_extract(review.material_json,'$.transcript.platform_account_id')=review.native_author_id
           AND json_extract(review.material_json,'$.transcript.status')=transcript.status
           AND json_extract(review.material_json,'$.transcript.segments')=json(transcript.segments_json)
           AND json_extract(review.material_json,'$.transcript.truncated')=transcript.truncated
           AND json_extract(review.material_json,'$.transcript.stop_code') IS transcript.stop_code
           AND json_extract(review.material_json,'$.transcript.call_number')=transcript.call_number
           AND json_extract(review.material_json,'$.transcript.captured_at')=transcript.updated_at))
       AND EXISTS(SELECT 1 FROM discovery_video_completions completed JOIN discovery_video_identity_bindings binding ON binding.search_hit_id=completed.search_hit_id
         WHERE completed.search_hit_id=h.id AND binding.discovery_campaign_id=d.id AND binding.criteria_revision=d.current_revision
           AND binding.candidate_id=c.id AND binding.native_author_id=review.native_author_id))
     OR EXISTS(SELECT 1 FROM discovery_content_reads original
       WHERE original.search_hit_id=h.id AND original.discovery_campaign_id=d.id
         AND original.expected_revision=d.current_revision AND original.platform=h.platform
         AND original.profile_ref=h.profile_ref AND original.status='completed' AND original.stop_code IS NULL
         AND json_extract(review.material_json,'$.native_content.status')='completed'
         AND json_extract(review.material_json,'$.native_content.stop_code') IS NULL
         AND json_extract(review.material_json,'$.native_content.claim.search_hit_id')=h.id
         AND json_extract(review.material_json,'$.native_content.claim.campaign_id')=d.id
         AND json_extract(review.material_json,'$.native_content.claim.expected_revision')=original.expected_revision
         AND json_extract(review.material_json,'$.native_content.claim.platform')=original.platform
         AND json_extract(review.material_json,'$.native_content.claim.profile_ref')=original.profile_ref
         AND json_extract(review.material_json,'$.native_content.claim.author_id')=original.author_id
         AND json_extract(review.material_json,'$.native_content.claim.source_url')=original.source_url
         AND json_extract(review.material_json,'$.native_content.claim.call_number')=original.call_number
         AND json_extract(review.material_json,'$.native_content.native_content')=json(original.native_content_json))
   )
   AND (d.max_calls IS NULL OR d.used_calls + 1 <= d.max_calls)
   AND EXISTS(SELECT 1 FROM json_each(review.findings_json) f WHERE json_extract(f.value,'$.criterion')='content_language' AND json_extract(f.value,'$.outcome')='insufficient')
   AND coalesce(json_array_length(review.material_json,'$.language_samples'),0) = (SELECT COUNT(*) FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision AND s.status='completed')
   AND (SELECT COUNT(*) FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision)<1
   AND (h.platform NOT IN ('instagram','weibo','bluesky','zhihu','hackernews') OR NOT EXISTS(SELECT 1 FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision))
   AND (h.platform!='facebook' OR EXISTS(SELECT 1 FROM discovery_facebook_content_targets t
     WHERE t.search_hit_id=h.id AND t.discovery_campaign_id=d.id AND t.expected_revision=d.current_revision
       AND t.status='completed' AND t.stop_code IS NULL
       AND json_array_length(t.source_urls_json)>(SELECT COUNT(*) FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision)))
   AND NOT EXISTS(SELECT 1 FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision AND s.status!='completed')
   AND EXISTS(SELECT 1 FROM discovery_platform_runs p WHERE p.discovery_campaign_id=d.id AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.status='completed')
) eligible
