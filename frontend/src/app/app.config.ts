import {
  ApplicationConfig,
  LOCALE_ID,
  isDevMode,
  provideBrowserGlobalErrorListeners,
  provideZonelessChangeDetection,
} from "@angular/core";
import { registerLocaleData } from "@angular/common";
import localeEnGb from "@angular/common/locales/en-GB";
import { provideHttpClient, withFetch } from "@angular/common/http";
import { provideRouter, withComponentInputBinding } from "@angular/router";
import { provideServiceWorker } from "@angular/service-worker";

import { routes } from "./app.routes";

// `DatePipe` reads LOCALE_ID, which Angular defaults to `en-US` whatever the
// browser is set to — so Trash and Sync conflicts told a UK reader a version was
// kept at "Sep 13, 2026, 10:12:45 AM". This is not the same knob as
// `toLocaleString()`, which follows the browser and was already right on a
// phone; the two disagreed inside one render and only this one was wrong for
// real users. Registering the data is required as well as naming the id: without
// it the pipe throws on any format that needs month or day names.
registerLocaleData(localeEnGb);

export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    { provide: LOCALE_ID, useValue: "en-GB" },
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding()),
    provideHttpClient(withFetch()),
    // Cache the app shell + read data so the app opens and shows your things
    // offline (prod build only). registerImmediately, not registerWhenStable:
    // the offline-first Buy list keeps the app "unstable" (its sync retries), so
    // waiting for stability would delay caching up to 30s — register now so the
    // cache is ready the moment you open the app (e.g. before the Tube).
    provideServiceWorker("ngsw-worker.js", {
      enabled: !isDevMode(),
      registrationStrategy: "registerImmediately",
    }),
  ],
};
