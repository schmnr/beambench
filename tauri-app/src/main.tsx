import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';
import { i18nReady } from './i18n';

function renderApp() {
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void i18nReady.then(renderApp, renderApp);
