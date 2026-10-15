/** Desktop builds alias ./webCaptions here (see app/vite.config.ts) so the
 *  browser Whisper worker and its wasm don't ship inside the desktop app. */
export type { Cue } from "./webCaptions";
const no = (): never => { throw new Error("Browser captions aren't part of the desktop build."); };
export const audioRange = no, cancelTranscribe = (): void => undefined, placeSegments = no, replaceCaptionCues = no, segmentsFromWords = no, transcribeWords = no;
