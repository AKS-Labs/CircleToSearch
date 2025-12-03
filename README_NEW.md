<!-- ---------- Header ---------- -->
<div align="center">
  <img src="app/src/main/res/drawable/circletosearch.png" width="180" height="180" style="border-radius: 20px; margin-bottom: 20px;">
  
  # Circle To Search
  
  <p><strong>Multi-Engine Reverse Image Search for Android</strong></p>
  <p>Draw a circle. Search instantly. <em>Google • Bing • Yandex • TinEye</em></p>

<!-- ---------- Badges ---------- -->
  <p>
    <img alt="License" src="https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat-square">
    <img alt="API" src="https://img.shields.io/badge/API%2029%2B-50f270?style=flat-square">
    <img alt="Kotlin" src="https://img.shields.io/badge/Kotlin-7F52FF?style=flat-square&logo=kotlin&logoColor=white">
    <img alt="Compose" src="https://img.shields.io/badge/Compose-4285F4?style=flat-square&logo=jetpack-compose&logoColor=white">
  </p>
  
  <p>
    <img alt="Downloads" src="https://img.shields.io/github/downloads/AKS-Labs/CircleToSearch/total?color=c3e7ff&style=flat-square">
    <img alt="Stars" src="https://img.shields.io/github/stars/AKS-Labs/CircleToSearch?color=c3e7ff&style=flat-square">
    <img alt="Last commit" src="https://img.shields.io/github/last-commit/AKS-Labs/CircleToSearch?color=c3e7ff&style=flat-square">
  </p>
</div>

---

## 🎯 What is Circle To Search?

Ever wanted to search for something you see on your phone screen? **Circle To Search** brings that power to your fingertips on *any* Android device. Simply draw a circle around what you're looking for, and instantly get results from your favorite search engine.

Think of it as having Google Lens, Bing Visual Search, Yandex, and TinEye all in one place—and it works everywhere.

---

## ✨ Key Features

| Feature | Description |
|---------|-------------|
| 🔄 **Multi-Engine Search** | Google Lens, Bing, Yandex, TinEye in one app |
| 🎨 **Intuitive Selection** | Draw a circle or scribble to select exactly what you want |
| ⚡ **Lightning Fast** | Smart preloading makes results appear instantly |
| 🌙 **Dark Mode** | Easy on the eyes, supports full dark mode |
| 🖥️ **Desktop/Mobile Modes** | Switch between desktop and mobile user agents |
| 🔒 **Privacy First** | No tracking, no analytics, fully open-source |
| 📱 **Universal** | Works on Android 10+ across all devices |

---

## 🚀 Quick Start

### Installation
Download from [Google Play Store](https://play.google.com/store/apps/details?id=com.akslabs.circletosearch) or build from source.

### First Time Setup
1. Open the app
2. Tap **"Enable Accessibility Service"** to grant permissions
3. Allow **accessibility** and **internet** permissions
4. You're ready to go!

### How to Use
1. **Trigger the overlay** by double-tapping the status bar
2. **Draw a circle** around the object or text you want to search
3. **Pick a search engine** from the bottom sheet
4. **Explore results** in the in-app browser

---

## 🏗️ Architecture

Built with modern Android best practices:

- **Kotlin** - Concise, safe, and expressive
- **Jetpack Compose** - Declarative UI framework for beautiful interfaces
- **Coroutines** - Efficient asynchronous operations
- **WebView** - Integrated browser for seamless search results
- **Accessibility API** - Non-intrusive screen capture and gesture detection

---

## 📸 Screenshots

| Feature | Preview |
|---------|---------|
| Circle Selection | Draw to select |
| Multi-Engine | Search across 4 engines |
| Results Browser | View results inline |
| Customization | Dark mode, desktop mode |

---

## 🔧 Development

### Requirements
- Android Studio (latest)
- Kotlin 1.9+
- Gradle 8.0+
- API 29+ (SDK Target: 36)

### Build & Run
```bash
git clone https://github.com/AKS-Labs/CircleToSearch.git
cd CircleToSearch
./gradlew installDebug
```

### Project Structure
```
app/
├── src/main/java/com/akslabs/circletosearch/
│   ├── ui/                 # Compose UI components
│   ├── data/               # Data models & search engines
│   ├── utils/              # Image processing & uploads
│   └── service/            # Accessibility service
├── res/
│   ├── drawable/           # Custom icons and assets
│   └── values/             # Strings, colors, themes
```

---

## 🤝 Contributing

Have an idea to make it better? We'd love to hear it!

1. **Fork** the repository
2. **Create** a feature branch: `git checkout -b feature/amazing-thing`
3. **Commit** your changes: `git commit -m 'Add amazing thing'`
4. **Push** to the branch: `git push origin feature/amazing-thing`
5. **Open** a Pull Request

---

## 📝 License

This project is licensed under the **Apache License 2.0**. See [LICENSE](LICENSE) for details.

---

## 💬 Connect

Questions? Feedback? Found a bug? Let's talk!

- **GitHub Issues** - [Report bugs or request features](https://github.com/AKS-Labs/CircleToSearch/issues)
- **GitHub Discussions** - [Share ideas and get help](https://github.com/AKS-Labs/CircleToSearch/discussions)
- **Telegram** - [@akslabs](https://t.me/akslabs)
- **GitHub** - [Follow for updates](https://github.com/aks-labs)

---

<div align="center">
  <p>
    <strong>Made with ❤️ by <a href="https://github.com/akslabs">AKS Labs</a></strong>
  </p>
  <p>
    <a href="https://github.com/sponsors/akslabs">☕ Support Development</a> • 
    <a href="https://github.com/AKS-Labs/CircleToSearch">⭐ Give a Star</a> • 
    <a href="https://github.com/AKS-Labs/CircleToSearch/fork">🍴 Fork & Contribute</a>
  </p>
  <p style="color: #666; margin-top: 30px; font-size: 12px;">
    Inspired by Google's Circle to Search • Open Source • Android Native
  </p>
</div>
