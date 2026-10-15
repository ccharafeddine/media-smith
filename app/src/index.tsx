import { render } from "solid-js/web";
import "@mediasmith/ui/fonts.css";
import "@mediasmith/ui/tokens.css";
import "@mediasmith/ui/components.css";
import "./editor.css";
import App from "./App";
import { applyIdentity } from "@mediasmith/ui";
import { configureFfmpeg } from "@mediasmith/platform";

// Smith v2 identity: violet hue, dark theme. Components opt in with sm- classes.
applyIdentity("mediasmith");
configureFfmpeg(import.meta.env.BASE_URL);

render(() => <App />, document.getElementById("root")!);
