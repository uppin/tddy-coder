/**
 * The one navigation double every playground spec shares: the screen reads and writes its
 * service/method selection through the navigation port, so the specs drive and read that port
 * instead of the window hash `tddy-web` binds it to.
 */
import { aNavigationDouble } from "tddy-client-web/testkit";

export const playgroundNavigation = aNavigationDouble("/rpc-playground");
