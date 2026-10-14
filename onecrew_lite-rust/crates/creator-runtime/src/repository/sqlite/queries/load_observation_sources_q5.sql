WITH source_ids(source_id) AS (
  SELECT observation.source_id
  FROM creator_account_links AS link
  JOIN platform_account_observations AS observation
    ON observation.platform_account_id = link.platform_account_id
  WHERE link.creator_id = :creator_id

  UNION

  SELECT origin.source_id
  FROM creator_origins AS origin
  WHERE origin.creator_id = :creator_id

  UNION

  SELECT locator_observation.source_id
  FROM locator_observations AS locator_observation
  WHERE locator_observation.creator_id = :creator_id

  UNION

  SELECT locator_observation.source_id
  FROM creator_account_links AS link
  JOIN locator_observations AS locator_observation
    ON locator_observation.platform_account_id = link.platform_account_id
  WHERE link.creator_id = :creator_id
)
SELECT source.id, source.kind, source.platform, source.source_url,
       source.external_ref, source.account_ref_id, source.metadata_json,
       source.observed_at
FROM source_ids
JOIN observation_sources AS source ON source.id = source_ids.source_id
ORDER BY source.observed_at DESC, source.id ASC;
