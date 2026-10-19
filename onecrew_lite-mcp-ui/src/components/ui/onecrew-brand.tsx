import officialLogo from "@mcp-ui/assets/onecrew-logo-black-text.svg?raw";

// Trusted, bundled official asset only. Its paths are unchanged; the wordmark
// follows Host theme while the yellow symbol retains its original color.
const logo = officialLogo.replace('width="740" height="161"', 'width="24" height="24" aria-hidden="true"').replace('viewBox="0 0 740 161"', 'viewBox="0 0 161 161"').replaceAll('fill="black"', 'fill="currentColor"');
export function OneCrewBrand() {
  return <span role="img" aria-label="onecrew_lite" data-onecrew-brand="true" className="onecrew-brand inline-flex items-center gap-2 shrink-0 align-middle"><span aria-hidden="true" dangerouslySetInnerHTML={{ __html: logo }} /><span className="font-semibold">onecrew_lite</span></span>;
}

export function OneCrewHeader() {
  return <div data-onecrew-header="true" className="onecrew-header"><OneCrewBrand /></div>;
}
