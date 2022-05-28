import { Event } from "@tauri-apps/api/helpers/event";
import { invoke } from "@tauri-apps/api/tauri";
import { appWindow } from "@tauri-apps/api/window";

import { Elm } from "./Main.elm";

const app = Elm.Main.init({ node: document.getElementById("root") });


if (window.__TAURI_IPC__) {
  app.ports.invoke.subscribe(([key, payload]) => {
    console.log(key, payload);

    invoke(key, payload);
  });


  appWindow.listen("new", (e) => {
    console.log(e);
    
  });
  appWindow.listen("get_languages", (e: Event<{
    languages: {
      name: string
    }[]
  }>) => {
    app.ports.getLanguages.send(e.payload.languages.map(({ name }) => ({ name })));
  });

  appWindow.listen("get_votes", (e: Event<{
    votes: {
      name: string
    }[]
  }>) => {

    console.log(e.payload.votes.map(({ name }) => ({ name })));

    app.ports.getVotes.send(e.payload.votes.map(({ name }) => ({ name })));
  });
  appWindow.emit("ping");
}
