import { lazy, Suspense } from "react";
import { Routes, Route, useLocation } from "react-router-dom";
import { AnimatePresence, motion } from "motion/react";
import Layout from "./components/Layout";
import { AuthProvider } from "./context/AuthContext";
import { TerminalProvider } from "./components/Terminal";
import { ToastProvider } from "./components/Toast";
import { pageTransition } from "./lib/motion";

const Dashboard = lazy(() => import("./pages/Dashboard"));
const TraderDetail = lazy(() => import("./pages/TraderDetail"));
const Activity = lazy(() => import("./pages/Activity"));
const MarketDetail = lazy(() => import("./pages/MarketDetail"));
const Alerts = lazy(() => import("./pages/Alerts"));
const Lab = lazy(() => import("./pages/Lab"));

export default function App() {
  const location = useLocation();

  return (
    <AuthProvider>
    <ToastProvider>
    <TerminalProvider>
    <Layout>
      <Suspense fallback={null}>
      <AnimatePresence mode="wait">
        <motion.div
          key={location.pathname}
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -10 }}
          transition={pageTransition}
        >
          <Routes location={location}>
            <Route path="/" element={<Dashboard />} />
            <Route path="/trader/:address" element={<TraderDetail />} />
            <Route path="/activity" element={<Activity />} />
            <Route path="/market/:tokenId" element={<MarketDetail />} />
            <Route path="/alerts" element={<Alerts />} />
            <Route path="/lab" element={<Lab />} />
          </Routes>
        </motion.div>
      </AnimatePresence>
      </Suspense>
    </Layout>
    </TerminalProvider>
    </ToastProvider>
    </AuthProvider>
  );
}
