import { createRouter, createWebHistory } from 'vue-router';
import {
  UserPage,
  SearchPage,
  HistoryPage,
  FavoritePage,
  DownPage,
  SettingsPage,
  InfoPage,
} from '@/views';

export const routes = [
  {
    path: '/user-page',
    name: 'userPage',
    component: UserPage,
  },
  {
    path: '/',
    name: 'searchPage',
    component: SearchPage,
  },
  {
    path: '/history-page',
    name: 'historyPage',
    component: HistoryPage,
  },
  {
    path: '/favorite-page',
    name: 'favoritePage',
    component: FavoritePage,
  },
  {
    path: '/down-page',
    name: 'downPage',
    component: DownPage,
  },
  {
    path: '/settings-page',
    name: 'settingsPage',
    component: SettingsPage,
  },
  {
    path: '/info-page',
    name: 'infoPage',
    component: InfoPage,
  },
] as const;

export default createRouter({
  history: createWebHistory(),
  routes,
});
