const COPY_ICON = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>`;

const CHECK_ICON = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"></polyline></svg>`;

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll('pre').forEach((pre) => {
    const code = pre.querySelector('code');
    if (!code) return;

    const button = document.createElement('button');
    button.className = 'copy-code-button';
    button.type = 'button';
    button.title = 'Copy';
    button.setAttribute('aria-label', 'Copy code');
    button.innerHTML = `<span class="copy-icon">${COPY_ICON}</span>`;
    pre.appendChild(button);

    button.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(code.innerText);
      } catch (err) {
        // Fallback for older browsers or denied permission.
        const textarea = document.createElement('textarea');
        textarea.value = code.innerText;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
      }

      button.innerHTML = `<span class="copy-icon copy-success">${CHECK_ICON}</span>`;
      setTimeout(() => {
        button.innerHTML = `<span class="copy-icon">${COPY_ICON}</span>`;
      }, 1500);
    });
  });
});
