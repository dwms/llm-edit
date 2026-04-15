function makeConfig() {
  const apiUrl = "https://api.example.com";
  return {
    url: () => apiUrl,
    host: () => new URL(apiUrl).host,
  };
}
