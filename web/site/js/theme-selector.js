// --- Theme dropdown logic ---
window.toggleThemeDropdown = function () {
    const dropdown = document.getElementById("theme-dropdown");
    const parent = dropdown.closest(".dropdown");
    const isOpen = parent.classList.contains("show");
    document
        .querySelectorAll(".dropdown.show")
        .forEach((d) => d.classList.remove("show"));
    if (!isOpen) parent.classList.add("show");
    dropdown.style.display =
        dropdown.style.display === "block" ? "none" : "block";
};

// --- Change Ace theme dynamically ---
function changeTheme(themeName) {
    const picolink = document.getElementById('picolink');
    picolink.setAttribute("href", `css/pico.${themeName}.min.css`);
    // change "Theme ▼" text to theme name after selection
    const btn = document.getElementById("theme-dropdown-btn");
    if (btn) btn.textContent = themeName + " ▼";
    // Hide dropdown after selection
    const dropdown = document.getElementById("theme-dropdown");
    if (dropdown) dropdown.style.display = "none";
    const parent = dropdown.closest(".dropdown");
    parent.classList.remove("show");

}
window.changeTheme = changeTheme;

// add event listener: when the user clicks outside the dropdown, stop showing it
document.addEventListener("click", function (event) {
    const dropdown = document.getElementById("theme-dropdown");
    const btn = document.getElementById("theme-dropdown-btn");
    if (!dropdown || !btn) return;
    const parent = dropdown.closest(".dropdown");
    if (
        !dropdown.contains(event.target) &&
        !btn.contains(event.target) &&
        parent.classList.contains("show")
    ) {
        parent.classList.remove("show");
        dropdown.style.display = "none";
    }
});