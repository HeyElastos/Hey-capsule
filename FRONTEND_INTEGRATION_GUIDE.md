# Modern Hey Frontend - Integration Guide

## 🎨 Frontend Architecture

### What's Been Created

A **modern Facebook-like interface** built with React 18, featuring:

#### Core Components
- **Layout System**: MainLayout with sticky Navbar, responsive Sidebar, and RightSidebar
- **Navigation**: Modern sidebar with icon-based menu (Home, Explore, Communities, Messages, Notifications, Profile, Saved)
- **Responsive Design**: Adapts beautifully from mobile to desktop
- **Tailwind CSS**: Modern utility-first styling with smooth animations

#### Key Features Implemented

**1. Feed System (Home Page)**
- Stories bar (Instagram-like)
- Create post modal with rich text options
- Post cards with like/comment/share/save functionality
- Real-time mock data with proper loading states

**2. User Profiles**
- Public user profiles (`/user/:userId`)
- Personal profile management (`/profile`)
- Follow/Unfollow functionality
- Profile stats (posts, followers, following)
- Tabs for posts, about, photos

**3. Communities**
- Browse and join communities
- Community detail pages with rules and members
- Leave community functionality
- Suggested communities

**4. Messages**
- Conversation list with unread indicators
- Chat interface with online status
- Send messages with timestamps
- Search conversations

**5. Notifications**
- Notification center with different types (likes, follows, comments, shares)
- Mark as read functionality
- Notification timestamps

**6. Explore**
- Search functionality
- Tabbed results (people, communities, posts)
- Discover mode

**7. Authentication**
- Modern login page with social options
- Sign up with validation
- Password visibility toggle
- Remember me functionality

#### Component Structure
```
src/
├── layouts/
│   └── MainLayout.jsx          # Main app layout
├── components/
│   ├── layout/
│   │   ├── Navbar.jsx          # Top navigation
│   │   ├── Sidebar.jsx         # Left navigation menu
│   │   └── RightSidebar.jsx    # Suggestions & online friends
│   ├── posts/
│   │   ├── CreatePost.jsx      # Post creation form
│   │   └── PostCard.jsx        # Individual post display
│   └── shared/
│       ├── PageLoader.jsx      # Loading animation
│       └── StoriesBar.jsx      # Instagram-like stories
├── pages/
│   ├── Home.jsx
│   ├── Profile.jsx
│   ├── UserProfile.jsx
│   ├── Communities.jsx
│   ├── CommunityDetail.jsx
│   ├── Messages.jsx
│   ├── Notifications.jsx
│   ├── Explore.jsx
│   ├── SavedPosts.jsx
│   ├── SignIn.jsx
│   ├── SignUp.jsx
│   └── NotFound.jsx
└── PrivateRoute.jsx            # Route protection
```

---

## 🔌 Backend Integration Points

### API Endpoints to Implement

#### 1. **Authentication** (`/auth`)
```javascript
// Already exist - just wire up frontend
POST /auth/signin         // Login
POST /auth/signup         // Register
POST /auth/refresh-token  // Token refresh
POST /auth/logout         // Logout
```

**Frontend Integration:**
- Update `SignIn.jsx` line 16-27: Replace TODO with actual API call
- Update `SignUp.jsx` line 47-60: Replace TODO with actual API call
- Create Redux actions for auth state management

#### 2. **Posts** (`/posts`)
```javascript
GET  /posts                              // Get user feed
GET  /posts/:publicUserId/userPosts      // Get user's posts
GET  /posts/:id                          // Get single post
POST /posts                              // Create post (with file upload)
PUT  /posts/:id                          // Update post
DELETE /posts/:id                        // Delete post
POST /posts/:id/like                     // Like post
DELETE /posts/:id/like                   // Unlike post
POST /posts/:id/comment                  // Add comment
DELETE /posts/:id/comment/:commentId    // Delete comment
POST /posts/:id/save                     // Save post
DELETE /posts/:id/unsave                 // Unsave post
```

**Files to Update:**
- `components/posts/CreatePost.jsx` (line 32): Post creation API call
- `components/posts/PostCard.jsx` (line 25, 65): Like/save API calls
- `pages/Home.jsx` (line 17): Fetch posts from `/posts/following`

#### 3. **Users** (`/users`)
```javascript
GET  /users/:id                  // Get user profile
GET  /users/public-users         // Get all users
GET  /users/public-users/:id     // Get public user profile
POST /users/follow/:userId       // Follow user
DELETE /users/follow/:userId     // Unfollow user
GET  /users/following            // Get following list
PUT  /users/:id                  // Update profile
POST /users/avatar               // Upload avatar
```

**Files to Update:**
- `pages/Profile.jsx` (line 23): Fetch current user data
- `pages/UserProfile.jsx` (line 20): Fetch user profile
- `components/layout/RightSidebar.jsx` (line 19): Fetch suggestions

#### 4. **Communities** (`/communities`)
```javascript
GET  /communities                      // Get all communities
GET  /communities/notmember            // Get communities user hasn't joined
GET  /communities/member               // Get communities user has joined
GET  /communities/:name                // Get community details
POST /communities                      // Create community
POST /communities/:name/join           // Join community
POST /communities/:name/leave          // Leave community
POST /communities/:name/report         // Report a post
GET  /communities/:name/members        // Get community members
GET  /communities/:name/moderators     // Get community moderators
```

**Files to Update:**
- `pages/Communities.jsx` (line 19): Fetch communities
- `pages/CommunityDetail.jsx` (line 22): Fetch community details

#### 5. **Notifications** (NEW - Needs Backend Support)
```javascript
GET  /notifications              // Get user notifications
POST /notifications/:id/read     // Mark as read
DELETE /notifications/:id        // Delete notification
```

**Files to Create:**
- Create notification service in `services/notificationService.js`
- Update `pages/Notifications.jsx` with real API calls

#### 6. **Messages** (NEW - Needs Backend Support)
```javascript
GET  /messages/conversations     // Get conversations
GET  /messages/:conversationId   // Get messages
POST /messages                   // Send message
```

**Files to Create:**
- Create message service in `services/messageService.js`
- Integrate WebSocket for real-time messaging

---

## 📋 TODO: Connect Frontend to Backend

### Phase 1: Essential APIs
- [ ] Update `redux/slices/authSlice.js` - Add login/signup actions
- [ ] Create API service layer: `services/apiService.js`
- [ ] Connect SignIn/SignUp to backend
- [ ] Implement token storage & refresh logic
- [ ] Connect Home feed to `/posts` endpoint
- [ ] Connect user profiles to `/users` endpoints
- [ ] Connect communities to `/communities` endpoints

### Phase 2: Features
- [ ] File upload for posts (avatar, images)
- [ ] Real-time notifications
- [ ] Real-time messaging with WebSocket
- [ ] Search functionality
- [ ] Comments on posts

### Phase 3: Polish
- [ ] Error handling & user feedback
- [ ] Loading states
- [ ] Infinite scroll for feeds
- [ ] Image optimization
- [ ] Performance optimization

---

## 🛠️ Installation & Setup

### Prerequisites
```bash
Node.js 16+
npm or yarn
```

### Install Dependencies
```bash
cd client
npm install
```

### Required New Package
```bash
npm install axios date-fns
```

### Environment Variables
```bash
# .env file
REACT_APP_API_BASE_URL=http://localhost:4000
REACT_APP_API_TIMEOUT=5000
```

### Start Development Server
```bash
npm start
# Opens at http://localhost:3000
```

---

## 🔐 Redux Setup Needed

Create/Update Redux slices for:

### `redux/slices/authSlice.js`
```javascript
import { createSlice } from '@reduxjs/toolkit';

const initialState = {
  userData: null,
  isAuthenticated: false,
  accessToken: null,
  loading: false,
  error: null,
};

export const authSlice = createSlice({
  name: 'auth',
  initialState,
  reducers: {
    setUser: (state, action) => { /* ... */ },
    logout: (state) => { /* ... */ },
    setLoading: (state, action) => { /* ... */ },
    setError: (state, action) => { /* ... */ },
  },
});
```

### `redux/slices/postSlice.js`
### `redux/slices/userSlice.js`
### `redux/slices/communitySlice.js`

---

## 📱 Responsive Breakpoints

- **Mobile**: < 640px (shows only Navbar and main content)
- **Tablet**: 640px - 1024px (Navbar + Main content)
- **Desktop**: 1024px - 1280px (Sidebar + Main content + RightSidebar hidden)
- **Large**: > 1280px (Sidebar + Main content + RightSidebar)

---

## 🎯 Features Already Wired

✅ Route navigation  
✅ Responsive layout  
✅ Form validation  
✅ Loading states  
✅ Mock data display  
✅ UI/UX interactions  

## ❌ Features Needing Backend Connection

❌ Authentication persistence  
❌ Real post data  
❌ Real user data  
❌ Real notifications  
❌ Real messaging  
❌ File uploads  
❌ Real-time updates  

---

## 📞 Quick Reference

### Main API Service Template
```javascript
// services/apiService.js
import axios from 'axios';

const API_BASE_URL = process.env.REACT_APP_API_BASE_URL || 'http://localhost:4000';

const api = axios.create({
  baseURL: API_BASE_URL,
  timeout: 5000,
});

// Add token to requests
api.interceptors.request.use((config) => {
  const token = localStorage.getItem('accessToken');
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

export default api;
```

### Usage Example
```javascript
// In component
import api from '../services/apiService';

useEffect(() => {
  api.get('/posts')
    .then(res => setPosts(res.data))
    .catch(err => setError(err.message));
}, []);
```

---

## 🚀 Next Steps

1. **Set up Redux store** with auth, posts, users, communities slices
2. **Create API service** with axios interceptors
3. **Connect authentication** - wire SignIn/SignUp
4. **Connect posts feed** - fetch from `/posts/following`
5. **Connect user profiles** - fetch from `/users/:id`
6. **Add error handling** - toast notifications or error boundaries
7. **Implement file uploads** - for posts and avatars
8. **Add real-time features** - notifications and messaging with Socket.io

---

## 💡 Design System

### Colors
- Primary: Blue (#2563EB)
- Secondary: Purple (#7C3AED)
- Success: Green (#10B981)
- Warning: Orange (#F59E0B)
- Error: Red (#EF4444)
- Light: Gray (#F3F4F6)
- Dark: Gray (#111827)

### Typography
- Font: System stack (sans-serif)
- Heading: Bold/Semibold
- Body: Regular
- Small: 12-14px
- Base: 16px
- Large: 18-20px

### Spacing
Uses Tailwind's standard scale (4px base unit)

---

## 📧 Backend Status

Your backend is **production-ready**! It has:
✅ User authentication with email verification
✅ Post system with comments and likes
✅ Communities with moderation
✅ Content analysis and filtering
✅ Admin panel
✅ Security features (rate limiting, CORS)

Just need to connect this modern frontend to it!
