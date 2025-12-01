(function () {
  if (!window.FIREBASE_CONFIG) {
    console.warn("FIREBASE_CONFIG is not defined; auth will be disabled.");
    return;
  }

  if (!window.firebase || !window.firebase.apps) {
    console.warn("Firebase SDK not loaded; auth will be disabled.");
    return;
  }

  if (firebase.apps.length === 0) {
    firebase.initializeApp(window.FIREBASE_CONFIG);
  }

  const auth = firebase.auth();

  function updateNav(user) {
    const el = document.getElementById("authControls");
    if (!el) return;
    if (!user) {
      el.innerHTML =
        '<button id="loginBtn" class="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800">' +
        '<span>🔐</span><span class="hidden sm:inline">Sign in</span></button>';
      const btn = document.getElementById("loginBtn");
      if (btn) {
        btn.addEventListener("click", () => {
          const provider = new firebase.auth.GoogleAuthProvider();
          auth.signInWithPopup(provider).catch((err) => {
            console.error("Login failed", err);
          });
        });
      }
    } else {
      const email = user.email || "Account";
      el.innerHTML =
        '<div class="flex items-center gap-3">' +
        '<span class="hidden sm:inline text-xs text-gray-400">' +
        email.replace(/</g, "&lt;") +
        "</span>" +
        '<button id="logoutBtn" class="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800">' +
        '<span>🚪</span><span class="hidden sm:inline">Sign out</span></button>' +
        "</div>";
      const btn = document.getElementById("logoutBtn");
      if (btn) {
        btn.addEventListener("click", () => {
          auth.signOut().catch((err) => {
            console.error("Logout failed", err);
          });
        });
      }
    }
  }

  auth.onAuthStateChanged((user) => {
    updateNav(user);
  });

  async function getCurrentIdToken(forceRefresh) {
    const user = auth.currentUser;
    if (!user) return null;
    return await user.getIdToken(!!forceRefresh);
  }

  window.authGetCurrentIdToken = getCurrentIdToken;
})();
