function makeConfig() {
  let apiUrl = "https://api.example.com";
  return {
    url: () => apiUrl,
    path: (p) => apiUrl + p,
  };
}
