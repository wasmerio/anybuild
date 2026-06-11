const message = document.getElementById("message");
const startButton = document.getElementById("startBtn");

startButton.addEventListener("click", () => {
  message.textContent = "Browser JavaScript is running.";
  document.body.classList.add("active");
});
