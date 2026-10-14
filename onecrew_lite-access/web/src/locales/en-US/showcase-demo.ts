const showcaseDemo = {
  addBrief: "Add brand brief",
  ariaLabel: "OneCrew AI creator search task demo",
  creatorDiscovery: "Creator discovery and outreach",
  creators: {
    ava: { channel: "Instagram · Beauty", detail: "North American audience · Strong engagement", handle: "@MiaMakes" },
    claire: { channel: "Instagram · Beauty", detail: "North American audience · Email available", handle: "@ClaireCreates" },
    ethan: { channel: "Instagram · Lifestyle", detail: "Strong brand fit", handle: "@EthanDaily" },
    imani: { channel: "YouTube · Reviews", detail: "Long-form video · Email available", handle: "@ImaniTests" },
    jonas: { channel: "YouTube · Everyday care", detail: "Long-form video · Email available", handle: "@JonasReviews" },
    lina: { channel: "TikTok · Clean skincare", detail: "Recently active · DMs open", handle: "@LinaGlow" },
    marcus: { channel: "YouTube · Ingredient reviews", detail: "Expert content · Worth contacting", handle: "@MarcusLab" },
    mei: { channel: "TikTok · Skincare", detail: "Tutorials · Contactable", handle: "@MeiRoutine" },
    nora: { channel: "Instagram · Ingredient reviews", detail: "Expert content · Worth contacting", handle: "@NoraNotes" },
    sofia: { channel: "TikTok · Skincare tutorials", detail: "Strong brand fit · High engagement", handle: "@SofiaSkin" },
  },
  promptStart: "Find creators for a North American skincare launch on TikTok and ",
  prompt: "Find creators for a North American skincare launch on TikTok and Instagram, then draft a personalized first message.",
  resultDescription: "Fit signals and available contact channels are ready to review",
  resultTitle: "48 creator matches found",
  runTask: "Run task",
} as const;

export default showcaseDemo;
