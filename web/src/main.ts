import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';
import './overview-layout.css';
import './tooltip.css';
mount(App, { target: document.getElementById('app')! });
