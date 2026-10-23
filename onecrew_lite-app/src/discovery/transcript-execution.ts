import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";

export function boundedDiscoveryTranscript(lines: readonly { timestamp: string; speaker: string; text: string }[]) {
  const segments: { timestamp: string; speaker: string; text: string }[] = [];
  for (const line of lines.slice(0, 200)) {
    segments.push({ timestamp: line.timestamp, speaker: line.speaker, text: line.text });
    if (Buffer.byteLength(stringifyApplicationJson(segments)) > 24000) {
      segments.pop(); if (!segments.length) throw new OpenCliError("opencli_output_too_large"); break;
    }
  }
  return { segments, truncated: segments.length < lines.length };
}
