import {
  ErrorHandler,
  ApplicationConfig,
  LOCALE_ID,
  isDevMode,
  provideBrowserGlobalErrorListeners,
  provideZonelessChangeDetection,
} from '@angular/core';
import { registerLocaleData } from '@angular/common';
import localeEnGb from '@angular/common/locales/en-GB';
import { provideHttpClient, withFetch, withInterceptors } from '@angular/common/http';
import { provideRouter, withComponentInputBinding } from '@angular/router';
import { provideServiceWorker } from '@angular/service-worker';

import { routes } from './app.routes';
import { TelemetryErrorHandler, failedRequestInterceptor } from './error-reporting';

// `DatePipe` reads LOCALE_ID, which Angular defaults to `en-US` whatever the
// browser says; `toLocaleString()` follows the browser. The locale data must be
// registered too, or the pipe throws on month and day names.
registerLocaleData(localeEnGb);

export const appConfig: ApplicationConfig = {
  providers: [
    { provide: ErrorHandler, useClass: TelemetryErrorHandler },
    provideZonelessChangeDetection(),
    { provide: LOCALE_ID, useValue: 'en-GB' },
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding()),
    provideHttpClient(withFetch(), withInterceptors([failedRequestInterceptor])),
    // Cache the app shell + read data so the app opens and shows your things
    // offline (prod build only). registerImmediately, not registerWhenStable:
    // the offline-first Buy list keeps the app "unstable" (its sync retries), so
    // waiting for stability would delay caching up to 30s — register now so the
    // cache is ready the moment you open the app (e.g. before the Tube).
    provideServiceWorker('ngsw-worker.js', {
      enabled: !isDevMode(),
      registrationStrategy: 'registerImmediately',
    }),
  ],
};
