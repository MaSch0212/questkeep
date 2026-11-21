import { render } from "solid-js/web";
import { App } from "./App";
import { installFocusRing } from "./lib/focusRing";
import { applyCachedAppearance } from "./lib/themes";
import "./theme.css";
import "./app.css";

// Before the first paint: the theme the last run ended with (the saved settings follow).
applyCachedAppearance();
installFocusRing();

const root = document.getElementById("root");
if (root) {
  render(() => <App />, root);
}
