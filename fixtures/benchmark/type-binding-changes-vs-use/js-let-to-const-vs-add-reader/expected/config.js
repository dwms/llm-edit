function makeConfig() {
  const apiUrl = "https://api.example.com";
  return {
    url: () => apiUrl,
    path: (p) => apiUrl + p,
  };
}
