import type { PageEntry } from "../lib/extensions/registry";
import { ExtensionMount } from "./ExtensionMount";

/** A page an extension added (`ui.addPage`): its title and what its `render` draws. */
export function PluginPage(props: { entry: PageEntry }) {
  return (
    <div class="plugin-page" data-plugin-page={props.entry.key}>
      <h1 class="plugin-page__title">{props.entry.page.title}</h1>
      <ExtensionMount
        plugin={props.entry.plugin}
        render={props.entry.page.render}
        class="plugin-page__body"
      />
    </div>
  );
}
