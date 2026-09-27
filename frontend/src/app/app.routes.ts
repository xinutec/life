import { Routes } from '@angular/router';

import { Conflicts } from './features/conflicts/conflicts';
import { EmotionCalendar } from './features/wellbeing/emotion-calendar';
import { House } from './features/house/house';
import { Inventory } from './features/inventory/inventory';
import { Items } from './features/items/items';
import { ProductPage } from './features/product/product';
import { Recipes } from './features/recipes/recipes';
import { Settings } from './features/settings/settings';
import { Shopping } from './features/shopping/shopping';
import { Todo } from './features/todo/todo';
import { Today } from './features/today/today';
import { Trash } from './features/trash/trash';
import { Wellbeing } from './features/wellbeing/wellbeing';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'today' },
  { path: 'today', title: 'Life · today', component: Today, data: { top: true } },
  { path: 'shopping', title: 'Life · buy', component: Shopping, data: { top: true } },
  { path: 'inventory', title: 'Life · inventory', component: Inventory, data: { top: true } },
  { path: 'recipes', title: 'Life · recipes', component: Recipes, data: { top: true } },
  { path: 'house', title: 'Life · house', component: House, data: { top: true } },
  { path: 'items', title: 'Life · all items', component: Items, data: { top: true } },
  {
    path: 'product/:id',
    title: 'Life · product',
    component: ProductPage,
    data: { up: { path: '/inventory', opener: true } },
  },
  { path: 'todo', title: 'Life · to-do', component: Todo, data: { top: true } },
  { path: 'wellbeing', title: 'Life · wellbeing', component: Wellbeing, data: { top: true } },
  {
    path: 'emotions',
    title: 'Life · emotion calendar',
    component: EmotionCalendar,
    data: { top: true },
  },
  {
    path: 'trash',
    title: 'Life · recently deleted',
    component: Trash,
    data: { up: { path: '/today', opener: true } },
  },
  {
    path: 'conflicts',
    title: 'Life · sync conflicts',
    component: Conflicts,
    data: { up: { path: '/today', opener: true } },
  },
  {
    path: 'settings',
    title: 'Life · settings',
    component: Settings,
    data: { up: { path: '/today', opener: true } },
  },
  { path: '**', redirectTo: 'today' },
];
