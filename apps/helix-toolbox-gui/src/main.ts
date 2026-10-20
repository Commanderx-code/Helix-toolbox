import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { keepScrollingReachable } from './lib/keys';
import { applyRemembered } from './lib/theme';

applyRemembered();

const app = mount(App, { target: document.getElementById('app')! });
keepScrollingReachable();

export default app;
