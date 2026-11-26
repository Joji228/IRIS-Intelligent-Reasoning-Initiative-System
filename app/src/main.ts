import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { installPressFlash } from './lib/press';
import { installStill } from './lib/still';

installPressFlash();
installStill();

const app = mount(App, { target: document.getElementById('app')! });

export default app;
