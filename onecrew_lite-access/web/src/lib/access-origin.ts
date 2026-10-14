// Access UI and Access API intentionally share one origin. Keeping these URLs
// relative prevents the public marketing origin from becoming an API caller.
export const accessOrigin = "";

export function accessApiURL(path: `/api/${string}`) {
  return path;
}
