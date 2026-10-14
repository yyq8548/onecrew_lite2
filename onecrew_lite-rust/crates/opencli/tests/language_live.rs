use onecrew_opencli::OpenCli;

#[tokio::test]
#[ignore = "one Instagram native search and one same-author original feed; explicit profile required"]
async fn instagram_recent_posts_preserve_observed_author_and_original_caption() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let query = std::env::var("OPENCLI_TEST_INSTAGRAM_QUERY").unwrap_or_else(|_| "shampoo".into());
    let rows = cli
        .creator_search(&profile, "instagram", &query, 1)
        .await
        .unwrap();
    let first = rows.first().unwrap();
    let author = first.author_id.as_deref().unwrap();
    let username = first
        .source_url
        .strip_prefix("https://www.instagram.com/")
        .unwrap()
        .strip_suffix('/')
        .unwrap();
    let started = std::time::Instant::now();
    let posts = cli
        .instagram_recent_original_posts(&profile, username, author)
        .await
        .unwrap_or_else(|error| panic!("Instagram originals failed; elapsed_ms={}; error={error}", started.elapsed().as_millis()));
    assert!(!posts.is_empty());
    assert!(posts.len() <= 3);
    assert!(posts.iter().all(|p| p.author_id == author));
    assert!(posts.iter().any(|p| !p.text.trim().is_empty()));
    println!("Instagram {} bound original captions", posts.len());
    for post in &posts {
        println!("original_caption={}", post.text);
    }
}

#[tokio::test]
#[ignore = "one bounded Instagram user search; explicit browser profile required"]
async fn instagram_search_retains_native_author_identity() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .creator_search(&profile, "instagram", "shampoo", 5)
        .await
        .unwrap();
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(
            row.author_id
                .as_ref()
                .is_some_and(|id| id.bytes().all(|b| b.is_ascii_digit()))
        );
        assert_eq!(row.author_url.as_deref(), Some(row.source_url.as_str()));
    }
    println!(
        "Instagram search returned {} bound native authors",
        rows.len()
    );
}

#[tokio::test]
#[ignore = "one Instagram search, same-author native profile and original captions; explicit profile required"]
async fn instagram_search_author_binds_to_unified_profile() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let query = std::env::var("OPENCLI_TEST_INSTAGRAM_QUERY").unwrap_or_else(|_| "shampoo".into());
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let started = std::time::Instant::now();
    let rows = cli
        .creator_search(&profile, "instagram", &query, 1)
        .await
        .unwrap();
    println!(
        "Instagram search PASS; elapsed_ms={}; rows={}",
        started.elapsed().as_millis(),
        rows.len()
    );
    let row = rows.first().expect("actual search candidate required");
    let author = row.author_id.as_deref().unwrap();
    let source = row.author_url.as_deref().unwrap();
    let started = std::time::Instant::now();
    let evidence = cli
        .creator_profile_evidence(&profile, "instagram", author, source)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "Instagram native profile failed; elapsed_ms={}; error={error}",
                started.elapsed().as_millis()
            )
        });
    assert_eq!(evidence["id"], author);
    assert_eq!(evidence["profile_url"], source);
    println!(
        "Instagram native profile PASS; elapsed_ms={}; exact_follower_count={}; public_email_present={}; public_links={}; no contact ownership or recommendation asserted",
        started.elapsed().as_millis(),
        evidence["follower_count"].is_u64(),
        evidence["public_email"]
            .as_str()
            .is_some_and(|value| !value.trim().is_empty()),
        evidence["contact_links"].as_array().map_or(0, Vec::len)
    );
    let username = source
        .strip_prefix("https://www.instagram.com/")
        .unwrap()
        .strip_suffix('/')
        .unwrap();
    let started = std::time::Instant::now();
    let posts = cli
        .instagram_recent_original_posts(&profile, username, author)
        .await
        .expect("same author original captions");
    assert!(!posts.is_empty() && posts.len() <= 3);
    assert!(posts.iter().all(|post| post.author_id == author));
    assert!(posts.iter().any(|post| !post.text.trim().is_empty()));
    println!(
        "Instagram native original PASS; elapsed_ms={}; captions={}; no contact or recommendation asserted",
        started.elapsed().as_millis(),
        posts.len()
    );
}

#[tokio::test]
#[ignore = "bounded TikTok public profile read; explicit observed author required"]
async fn tiktok_profile_returns_bound_native_evidence() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let username = std::env::var("OPENCLI_TEST_TIKTOK_USERNAME").unwrap();
    let author = std::env::var("OPENCLI_TEST_TIKTOK_AUTHOR").unwrap();
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let row = cli
        .tiktok_public_profile(&profile, &username, &author)
        .await
        .unwrap();
    assert_eq!(row.id, author);
    assert_eq!(row.username, username);
    println!(
        "TikTok bound public profile returned; followers={}",
        row.follower_count
    );
}

#[tokio::test]
#[ignore = "Instagram public DOM; explicit observed username and saved search author ID required"]
async fn instagram_profile_preserves_native_identity_and_count() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let username = std::env::var("OPENCLI_TEST_INSTAGRAM_USERNAME").unwrap();
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let result = cli
        .instagram_public_profile(&profile, &username, &std::env::var("OPENCLI_TEST_INSTAGRAM_AUTHOR_ID").unwrap())
        .await
        .unwrap();
    assert_eq!(result.username, username.to_ascii_lowercase());
    println!(
        "Instagram native author={} followers={:?} source={}",
        result.id, result.follower_count, result.profile_url
    );
}

#[tokio::test]
#[ignore = "bounded Facebook original post and public comments; explicit observed source required"]
async fn facebook_comments_preserve_original_language() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let source = std::env::var("OPENCLI_TEST_FACEBOOK_SOURCE").unwrap();
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::FacebookPost(&source),
            5,
        )
        .await
        .unwrap();
    assert_eq!(rows[0]["type"], "POST");
    assert_eq!(rows[0]["source_url"], source);
    assert!((2..=6).contains(&rows.len()));
    for row in &rows[1..] {
        assert_eq!(row["type"], "COMMENT");
        assert!(
            row["source_url"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{source}?comment_id="))
        );
        assert!(row.get("native_author_id").is_none());
        assert!(row.get("author_id").is_none());
        assert!(!row["text"].as_str().unwrap().is_empty());
    }
    println!(
        "Facebook original post and {} visible comments; no commenter identities",
        rows.len() - 1
    );
}

#[tokio::test]
#[ignore = "TikTok bounded recent locators and one real scoring sample; explicit profile required"]
async fn tiktok_language_material_round_trip() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let source = std::env::var("OPENCLI_TEST_TIKTOK_SOURCE").unwrap();
    let username = std::env::var("OPENCLI_TEST_TIKTOK_USERNAME").unwrap();
    let author = std::env::var("OPENCLI_TEST_TIKTOK_AUTHOR").unwrap();
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let urls = cli
        .tiktok_recent_language_urls(&profile, &username, &author)
        .await
        .unwrap();
    assert!((1..=3).contains(&urls.len()));
    let sample = cli
        .tiktok_language_sample(&profile, &username, &author, &source)
        .await
        .unwrap();
    assert_eq!(sample["author_id"], author);
    assert_eq!(sample["source_url"], source);
    let comments = sample["comments"].as_array().unwrap();
    assert!(!comments.is_empty());
    assert!(
        comments
            .iter()
            .all(|c| c.get("author_id").is_none()
                && c["text"].as_str().is_some_and(|s| !s.is_empty()))
    );
    println!(
        "TikTok: {} bounded post locators and {} language comments without commenter IDs",
        urls.len(),
        comments.len()
    );
}

#[tokio::test]
#[ignore = "bounded Instagram public comments; explicit profile and observed post required"]
async fn instagram_comments_preserve_post_and_comment_locators() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let source = std::env::var("OPENCLI_TEST_INSTAGRAM_SOURCE").expect("observed source");
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::InstagramPost(&source),
            5,
        )
        .await
        .expect("public comments");
    assert_eq!(rows[0]["type"], "POST");
    assert_eq!(rows[0]["source_url"], source);
    assert!((2..=6).contains(&rows.len()));
    for row in &rows[1..] {
        assert_eq!(row["type"], "COMMENT");
        assert!(
            row["id"]
                .as_str()
                .unwrap()
                .bytes()
                .all(|b| b.is_ascii_digit())
        );
        assert!(row["source_url"].as_str().unwrap().contains("/c/"));
        assert!(row["native_author_id"].is_null());
        assert!(!row["text"].as_str().unwrap().trim().is_empty());
    }
    println!(
        "Instagram original caption and {} comments with same-post locators; stable account IDs unknown",
        rows.len() - 1
    );
}

#[tokio::test]
#[ignore = "bounded TikTok public comments; requires an observed video and explicit browser profile"]
async fn tiktok_comments_preserve_original_post_and_language_text() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let source = std::env::var("OPENCLI_TEST_TIKTOK_SOURCE").expect("observed video URL");
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::TiktokVideo(&source),
            5,
        )
        .await
        .expect("public comments");
    assert_eq!(rows[0]["type"], "POST");
    assert_eq!(rows[0]["source_url"], source);
    assert!(
        rows[0]["native_author_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(!rows[0]["text"].as_str().unwrap().trim().is_empty());
    assert!((2..=6).contains(&rows.len()));
    for row in &rows[1..] {
        assert_eq!(row["type"], "COMMENT");
        assert_eq!(row["source_url"], source);
        assert!(row.get("native_author_id").is_none());
        assert!(row.get("author_url").is_none());
        assert!(!row["text"].as_str().unwrap().trim().is_empty());
    }
    println!(
        "TikTok source-bound original post, {} public comments without commenter IDs",
        rows.len() - 1
    );
}

#[tokio::test]
#[ignore = "bounded public Weibo post and comments; requires observed source URL and connected profile"]
async fn weibo_comments_preserve_native_identities() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let source = std::env::var("OPENCLI_TEST_WEIBO_SOURCE").expect("observed source URL");
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let post = cli
        .weibo_post_evidence(&profile, &source)
        .await
        .expect("source-bound post");
    assert_eq!(post.source_url, source);
    assert!(!post.text.trim().is_empty());
    println!(
        "Observed Weibo post={}, author={}",
        post.post_id, post.author_id
    );
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::WeiboPost(&post.post_id),
            5,
        )
        .await
        .expect("native comment read");
    assert!(
        !rows.is_empty(),
        "observed post must supply actual comments"
    );
    assert!(rows.len() <= 5);
    assert!(rows.iter().all(|row| {
        row["id"].as_str().is_some_and(|id| !id.is_empty())
            && row["text"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty())
            && row["text_truncated"].is_boolean()
    }));
    assert!(rows.iter().any(|row| row["native_author_id"].is_string()));
    println!(
        "Weibo source-bound post={}, author={}; comments={}; no Campaign write",
        post.post_id,
        post.author_id,
        rows.len()
    );
}

#[tokio::test]
#[ignore = "bounded Zhihu comment read of an explicitly observed answer; no Campaign writes"]
async fn zhihu_comments_return_textual_replies() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let answer = std::env::var("OPENCLI_TEST_ZHIHU_ANSWER").expect("observed answer ID");
    let cli = OpenCli::inspect_system().await.unwrap();
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::ZhihuAnswer(&answer),
            5,
        )
        .await
        .expect("native comment read");
    assert!(
        !rows.is_empty(),
        "observed answer must supply actual comments"
    );
    assert!(rows.len() <= 5);
    assert!(rows.iter().all(|row| {
        row["content"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
    }));
    let identified = rows
        .iter()
        .filter(|row| {
            row["native_author_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        })
        .count();
    assert!(
        identified > 0,
        "observed sample must retain native commenter IDs"
    );
    println!(
        "Zhihu textual comments={}, native commenter IDs={}; answer-owner comparison not yet proven",
        rows.len(),
        identified
    );
}

#[tokio::test]
#[ignore = "bounded Reddit comment read of an explicitly observed post; no Campaign writes"]
async fn reddit_comments_return_textual_replies() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let post = std::env::var("OPENCLI_TEST_REDDIT_POST").expect("observed post ID");
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::RedditPost(&post),
            5,
        )
        .await
        .expect("native comment read");
    let comments: Vec<_> = rows.iter().filter(|row| row["type"] == "L0").collect();
    assert!(
        !comments.is_empty(),
        "observed post must supply actual comments"
    );
    assert!(comments.len() <= 5);
    assert!(
        comments
            .iter()
            .all(|row| { row["text"].as_str().is_some_and(|s| !s.trim().is_empty()) })
    );
    assert_eq!(rows[0]["id"], format!("t3_{post}"));
    assert!(
        rows[0]["native_author_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("t2_"))
    );
    assert!(
        comments
            .iter()
            .all(|row| row["id"].as_str().is_some_and(|id| id.starts_with("t1_")))
    );
    assert!(
        comments
            .iter()
            .all(|row| row.get("native_author_id").is_none())
    );
    println!(
        "Reddit source-bound original post and {} language comments without commenter IDs",
        comments.len()
    );
}

#[tokio::test]
#[ignore = "two bounded public Reddit reads; observed post author then bound native profile"]
async fn reddit_profile_matches_observed_post_author() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let post = std::env::var("OPENCLI_TEST_REDDIT_POST").expect("observed post ID");
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::RedditPost(&post),
            1,
        )
        .await
        .unwrap();
    let author = rows[0]["native_author_id"].as_str().expect("native author");
    let username = rows[0]["author"].as_str().expect("observed username");
    let result = cli
        .creator_profile_evidence(
            &profile,
            "reddit",
            author,
            &format!("https://www.reddit.com/user/{username}/"),
        )
        .await
        .unwrap();
    let result: onecrew_opencli::RedditPublicProfile = serde_json::from_value(result).unwrap();
    assert_eq!(result.id, author);
    assert!(result.username.eq_ignore_ascii_case(username));
    println!(
        "Reddit post/profile native identity matched; followers={:?}; no Campaign write",
        result.follower_count
    );
}

#[tokio::test]
#[ignore = "three bounded Reddit reads for original language, comments and recent author posts"]
async fn reddit_language_sample_and_recent_posts_are_bound() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let post = std::env::var("OPENCLI_TEST_REDDIT_POST").expect("observed post ID");
    let mut cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
        cli = OpenCli::inspect_system().await.unwrap();
    }
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::RedditPost(&post),
            1,
        )
        .await
        .unwrap();
    let author = rows[0]["native_author_id"].as_str().expect("native author");
    let username = rows[0]["author"].as_str().expect("observed username");
    let source = format!("https://www.reddit.com/comments/{post}/");
    let sample = cli
        .reddit_language_sample(&profile, username, author, &source)
        .await
        .unwrap();
    assert_eq!(sample["author_id"], author);
    assert_eq!(sample["source_url"], source);
    let comments = sample["comments"].as_array().unwrap();
    assert!(!comments.is_empty() && comments.len() <= 20);
    assert!(
        comments
            .iter()
            .all(|row| row.as_object().unwrap().len() == 2)
    );
    let urls = cli
        .reddit_recent_language_urls(&profile, username, author)
        .await
        .unwrap();
    assert!(!urls.is_empty() && urls.len() <= 3);
    println!(
        "Reddit author-bound original/comments={}; recent={}; no Campaign write",
        comments.len(),
        urls.len()
    );
}

#[tokio::test]
#[ignore = "two bounded original-post reads; requires explicit connected profile and post"]
async fn reddit_initial_content_matches_native_author() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let post = std::env::var("OPENCLI_TEST_REDDIT_POST").expect("observed post");
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let cli = OpenCli::inspect_system().await.unwrap();
    let rows = cli
        .native_comments(
            &profile,
            onecrew_opencli::NativeCommentSource::RedditPost(&post),
            1,
        )
        .await
        .unwrap();
    let author = rows[0]["native_author_id"].as_str().unwrap();
    let source = format!("https://www.reddit.com/comments/{post}/");
    let content = cli
        .reddit_post_content(&profile, author, &source)
        .await
        .unwrap();
    assert_eq!(content["author_id"], author);
    assert_eq!(content["source_url"], source);
    assert!(!content["text"].as_str().unwrap().trim().is_empty());
    assert_eq!(content.as_object().unwrap().len(), 5);
    assert!(content.get("comments").is_none());
    println!(
        "Reddit original post bound; bytes={}; no comments promoted; no Campaign write",
        content["text"].as_str().unwrap().len()
    );
}

#[tokio::test]
#[ignore = "bounded public Bilibili reads; requires explicit connected profile and installed add-ons"]
async fn bilibili_language_preserves_observed_author_and_comments() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let mid = std::env::var("OPENCLI_TEST_BILIBILI_MID").expect("observed native author ID");
    let video = std::env::var("OPENCLI_TEST_BILIBILI_BVID").expect("observed native video ID");
    let source = format!("https://www.bilibili.com/video/{video}");
    let sample = cli
        .bilibili_language_sample(&profile, &mid, &source)
        .await
        .unwrap();
    assert_eq!(sample["author_id"], mid);
    assert_eq!(sample["source_url"], source);
    assert!(
        sample["text"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
    );
    let comments = sample["comments"].as_array().expect("native comments");
    assert!(comments.len() <= 20);
    assert!(
        !comments.is_empty(),
        "observed public video has textual comments"
    );
    println!(
        "Bilibili author bound; comments={}; no Campaign write",
        comments.len()
    );
}

#[tokio::test]
#[ignore = "one bounded recent-video read; requires connected profile and observed native author"]
async fn bilibili_recent_language_urls_are_available() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let mid = std::env::var("OPENCLI_TEST_BILIBILI_MID").expect("observed native author ID");
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let recent = cli
        .bilibili_recent_language_urls(&profile, &mid)
        .await
        .unwrap();
    assert!(!recent.is_empty() && recent.len() <= 3);
    println!(
        "Bilibili recent author-bound URLs={}; no Campaign write",
        recent.len()
    );
}

#[tokio::test]
#[ignore = "two bounded reads of an explicitly supplied public post; requires connected profile"]
async fn x_translated_post_returns_original_language_evidence() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE").expect("explicit handle");
    let post = std::env::var("OPENCLI_TEST_POST_ID").expect("explicit post");
    let expected = std::env::var("OPENCLI_TEST_ORIGINAL_TEXT").expect("observed original text");
    let cli = OpenCli::inspect_system().await.unwrap();
    let identity = cli
        .twitter_post_identity(&profile, &handle, &post)
        .await
        .unwrap();
    let sample = cli
        .twitter_language_sample(&profile, &handle, &post, &identity.account_id)
        .await
        .unwrap();
    assert_eq!(sample["text"].as_str(), Some(expected.as_str()));
    assert_eq!(sample["author_id"], identity.account_id);
    let comments = sample["comments"].as_array().expect("native replies");
    assert!(comments.len() <= 20);
    println!(
        "Exact original matched; replies={}; no Campaign write",
        comments.len()
    );
}

#[tokio::test]
#[ignore = "three bounded public reads; requires explicit connected profile and public handle"]
async fn x_language_sample_uses_observed_post_and_native_author() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE").expect("explicit public handle");
    let cli = OpenCli::inspect_system().await.unwrap();
    let recent = cli.twitter_recent_content(&profile, &handle).await.unwrap();
    let post = recent
        .original_posts
        .first()
        .expect("an observed original post");
    let identity = cli
        .twitter_post_identity(&profile, &handle, &post.id)
        .await
        .unwrap();
    let sample = cli
        .twitter_language_sample(&profile, &handle, &post.id, &identity.account_id)
        .await
        .unwrap();
    assert_eq!(sample["author_id"], identity.account_id);
    assert_eq!(sample["source_url"], identity.source_url);
    assert!(
        sample["text"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
    );
    let comments = sample["comments"]
        .as_array()
        .expect("native direct replies");
    assert!(comments.len() <= 20);
    println!(
        "Original post bytes={}, direct replies={}; no Campaign write",
        sample["text"].as_str().unwrap().len(),
        comments.len()
    );
}
#[tokio::test]
#[ignore = "one bounded public search; requires explicit connected profile"]
async fn reddit_search_retains_observed_native_authors() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let cli = OpenCli::inspect_system().await.unwrap();
    let hits = cli
        .creator_search(&profile, "reddit", "shampoo", 5)
        .await
        .unwrap();
    assert!(!hits.is_empty() && hits.len() <= 5);
    let bound = hits
        .iter()
        .filter(|hit| {
            hit.author_id
                .as_deref()
                .is_some_and(|id| id.starts_with("t2_"))
                && hit
                    .author_url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("https://www.reddit.com/user/"))
        })
        .count();
    assert!(
        bound > 0,
        "native author IDs must survive search projection"
    );
    println!(
        "Reddit search rows={}; native author pairs={bound}; no Campaign write",
        hits.len()
    );
}
